// Apple availability checks live here so the same binary can run on macOS 13.
#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <CoreAudio/CoreAudio.h>
#import <CoreAudio/CATapDescription.h>
#import <CoreAudio/AudioHardwareTapping.h>
#import <CoreMedia/CoreMedia.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#include <libproc.h>
#include <signal.h>
#include <errno.h>
#include <stdatomic.h>

typedef struct {
    const float *data;
    uint32_t samples;
    uint32_t channels;
} ParlanceBuffer;
typedef void (*ParlancePush)(void *, const ParlanceBuffer *, uint32_t);

static OSStatus get_property(AudioObjectID object, AudioObjectPropertySelector selector,
                             AudioObjectPropertyScope scope, UInt32 *size, void *value) {
    AudioObjectPropertyAddress address = {selector, scope, kAudioObjectPropertyElementMain};
    return AudioObjectGetPropertyData(object, &address, 0, NULL, size, value);
}

static NSArray<NSNumber *> *objects(AudioObjectPropertySelector selector) {
    AudioObjectPropertyAddress address = {selector, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain};
    UInt32 size = 0;
    if (AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &address, 0, NULL, &size)) return @[];
    AudioObjectID *ids = malloc(size);
    if (!ids) return @[];
    NSMutableArray *result = [NSMutableArray new];
    if (!AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, 0, NULL, &size, ids)) {
        for (NSUInteger i = 0; i < size / sizeof(AudioObjectID); ++i) [result addObject:@(ids[i])];
    }
    free(ids);
    return result;
}

static NSString *string_property(AudioObjectID object, AudioObjectPropertySelector selector) {
    CFStringRef value = NULL;
    UInt32 size = sizeof(value);
    if (get_property(object, selector, kAudioObjectPropertyScopeGlobal, &size, &value)) return nil;
    return CFBridgingRelease(value);
}

static AudioObjectID default_device(BOOL input) {
    AudioObjectID device = kAudioObjectUnknown;
    UInt32 size = sizeof(device);
    get_property(kAudioObjectSystemObject, input ? kAudioHardwarePropertyDefaultInputDevice :
                 kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal, &size, &device);
    return device;
}

static BOOL input_format(AudioObjectID device, AudioStreamBasicDescription *format) {
    UInt32 size = sizeof(*format);
    return !get_property(device, kAudioDevicePropertyStreamFormat, kAudioObjectPropertyScopeInput, &size, format)
        && format->mChannelsPerFrame > 0;
}

static BOOL input_buffer_count(AudioObjectID device, UInt32 *count) {
    AudioObjectPropertyAddress address = {kAudioDevicePropertyStreamConfiguration,
        kAudioObjectPropertyScopeInput, kAudioObjectPropertyElementMain};
    UInt32 size = 0;
    if (AudioObjectGetPropertyDataSize(device, &address, 0, NULL, &size) || size < sizeof(UInt32)) return NO;
    AudioBufferList *list = malloc(size);
    if (!list) return NO;
    BOOL success = !AudioObjectGetPropertyData(device, &address, 0, NULL, &size, list);
    if (success) *count = list->mNumberBuffers;
    free(list);
    return success;
}

static BOOL float_format(const AudioStreamBasicDescription *format) {
    return format->mFormatID == kAudioFormatLinearPCM && (format->mFormatFlags & kAudioFormatFlagIsFloat)
        && !(format->mFormatFlags & kAudioFormatFlagIsBigEndian) && format->mBitsPerChannel == 32
        && format->mChannelsPerFrame > 0 && format->mChannelsPerFrame <= 32 && format->mSampleRate > 0;
}

static BOOL in_process_tree(pid_t pid, pid_t root) {
    for (unsigned depth = 0; pid > 1 && depth < 64; ++depth) {
        if (pid == root) return YES;
        struct proc_bsdinfo info = {0};
        if (proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info)) != sizeof(info)) break;
        if ((pid_t)info.pbi_ppid == pid) break;
        pid = (pid_t)info.pbi_ppid;
    }
    return NO;
}

static NSArray<NSNumber *> *audio_processes(pid_t root) API_AVAILABLE(macos(14.2)) {
    NSMutableArray *result = [NSMutableArray new];
    for (NSNumber *object in objects(kAudioHardwarePropertyProcessObjectList)) {
        pid_t pid = 0;
        UInt32 size = sizeof(pid);
        if (!get_property(object.unsignedIntValue, kAudioProcessPropertyPID,
                          kAudioObjectPropertyScopeGlobal, &size, &pid) && in_process_tree(pid, root)) {
            [result addObject:object];
        }
    }
    return result;
}

char *parlance_macos_sources(void) {
    @autoreleasepool {
        NSMutableArray *sources = [NSMutableArray arrayWithObject:@{
            @"id": @"system:default", @"name": @"System audio", @"kind": @{@"type": @"systemOutput"}, @"isDefault": @YES
        }];
        AudioObjectID defaultInput = default_device(YES);
        for (NSNumber *number in objects(kAudioHardwarePropertyDevices)) {
            AudioObjectID device = number.unsignedIntValue;
            AudioStreamBasicDescription format = {0};
            if (!input_format(device, &format)) continue;
            NSString *uid = string_property(device, kAudioDevicePropertyDeviceUID);
            if (!uid) continue;
            [sources addObject:@{
                @"id": [@"mic:" stringByAppendingString:uid],
                @"name": string_property(device, kAudioObjectPropertyName) ?: @"Microphone",
                @"kind": @{@"type": @"microphone"}, @"isDefault": [NSNumber numberWithBool:device == defaultInput]
            }];
        }
        for (NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
            if (app.processIdentifier == getpid() || app.activationPolicy != NSApplicationActivationPolicyRegular) continue;
            [sources addObject:@{
                @"id": [NSString stringWithFormat:@"app:%d", app.processIdentifier],
                @"name": app.localizedName ?: @"Application", @"kind": @{@"type": @"application", @"pid": @(app.processIdentifier)},
                @"isDefault": @NO
            }];
        }
        NSData *json = [NSJSONSerialization dataWithJSONObject:sources options:0 error:nil];
        return json ? strndup(json.bytes, json.length) : NULL;
    }
}

@interface ParlanceCapture : NSObject <SCStreamOutput, SCStreamDelegate> {
@public
    void *_context;
    ParlancePush _push;
    atomic_bool _active;
    atomic_int _failure;
    AudioObjectID _device;
    AudioObjectID _tap;
    AudioObjectID _outputDevice;
    UInt32 _inputBufferOffset;
    AudioDeviceIOProcID _proc;
    BOOL _aggregate;
    BOOL _defaultInput;
    pid_t _pid;
    AudioStreamBasicDescription _format;
    SCStream *_stream;
    dispatch_queue_t _queue;
}
- (void)stop;
@end

// No Objective-C calls, allocations, locks or I/O on the Core Audio callback.
static void deliver(ParlanceCapture *capture, const AudioBufferList *list) {
    if (!atomic_load_explicit(&capture->_active, memory_order_acquire)) return;
    if (list->mNumberBuffers <= capture->_inputBufferOffset) return;
    UInt32 count = list->mNumberBuffers - capture->_inputBufferOffset;
    if (count > 32) { atomic_store(&capture->_failure, 1); return; }
    ParlanceBuffer buffers[32];
    uint32_t channels = 0;
    for (UInt32 i = 0; i < count; ++i) {
        const AudioBuffer *buffer = &list->mBuffers[capture->_inputBufferOffset + i];
        if (!buffer->mData || buffer->mDataByteSize == 0) return;
        buffers[i] = (ParlanceBuffer){buffer->mData, buffer->mDataByteSize / sizeof(float), buffer->mNumberChannels};
        channels += buffer->mNumberChannels;
    }
    if (channels != capture->_format.mChannelsPerFrame) { atomic_store(&capture->_failure, 1); return; }
    capture->_push(capture->_context, buffers, count);
}

static OSStatus audio_callback(AudioObjectID device, const AudioTimeStamp *now, const AudioBufferList *input,
                               const AudioTimeStamp *inputTime, AudioBufferList *output,
                               const AudioTimeStamp *outputTime, void *context) {
    (void)device; (void)now; (void)inputTime; (void)output; (void)outputTime;
    deliver((__bridge ParlanceCapture *)context, input);
    return noErr;
}

@implementation ParlanceCapture
- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
    atomic_store(&_failure, 2);
}
- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sample ofType:(SCStreamOutputType)type {
    if (type != SCStreamOutputTypeAudio || !atomic_load_explicit(&_active, memory_order_acquire)) return;
    const AudioStreamBasicDescription *format = CMAudioFormatDescriptionGetStreamBasicDescription(CMSampleBufferGetFormatDescription(sample));
    if (!format || !float_format(format) || format->mSampleRate != _format.mSampleRate ||
        format->mChannelsPerFrame != _format.mChannelsPerFrame) { atomic_store(&_failure, 1); return; }
    CMBlockBufferRef block = CMSampleBufferGetDataBuffer(sample);
    size_t contiguous = 0, total = 0;
    char *data = NULL;
    if (!block || CMBlockBufferGetDataPointer(block, 0, &contiguous, &total, &data) || contiguous != total) {
        atomic_store(&_failure, 1); return;
    }
    CMItemCount frames = CMSampleBufferGetNumSamples(sample);
    size_t channels = format->mChannelsPerFrame;
    if (frames <= 0 || total != (size_t)frames * channels * sizeof(float)) return;
    ParlanceBuffer buffers[32];
    BOOL planar = (format->mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0;
    uint32_t count = planar ? (uint32_t)channels : 1;
    for (uint32_t i = 0; i < count; ++i) {
        buffers[i] = (ParlanceBuffer){(const float *)data + (planar ? i * frames : 0),
                                     (uint32_t)(planar ? frames : frames * channels), planar ? 1 : (uint32_t)channels};
    }
    _push(_context, buffers, count);
}
- (void)stop {
    atomic_store_explicit(&_active, false, memory_order_release);
    if (_proc) {
        AudioDeviceStop(_device, _proc);
        AudioDeviceDestroyIOProcID(_device, _proc);
        _proc = NULL;
    }
    if (_stream) {
        // Removing output and draining its serial queue fences all Rust callback access.
        [_stream removeStreamOutput:self type:SCStreamOutputTypeAudio error:nil];
        dispatch_sync(_queue, ^{});
        [_stream stopCaptureWithCompletionHandler:^(NSError *error) { (void)error; }];
        _stream = nil;
    }
    if (_aggregate && _device) { AudioHardwareDestroyAggregateDevice(_device); _device = 0; }
    if (@available(macOS 14.2, *)) {
        if (_tap) { AudioHardwareDestroyProcessTap(_tap); _tap = 0; }
    }
}
- (void)dealloc { [self stop]; }
@end

static NSString *status_message(NSString *action, OSStatus status) {
    return [NSString stringWithFormat:@"%@: Core Audio error %d. Check System Settings > Privacy & Security > Screen & System Audio Recording.", action, (int)status];
}

static BOOL start_tap(ParlanceCapture *capture, pid_t pid, NSString **error, int *code) API_AVAILABLE(macos(14.2)) {
    NSArray *processes = audio_processes(pid ? pid : getpid());
    if (pid && processes.count == 0) { *error = @"The application has no audio stream. Play audio in it and try again."; return NO; }
    CATapDescription *description = pid ? [[CATapDescription alloc] initStereoMixdownOfProcesses:processes] :
        [[CATapDescription alloc] initStereoGlobalTapButExcludeProcesses:processes];
    description.privateTap = YES;
    description.muteBehavior = CATapUnmuted;
    description.name = @"Parlance audio capture";
    OSStatus status = AudioHardwareCreateProcessTap(description, &capture->_tap);
    if (status) {
        if (status == kAudioDevicePermissionsError || status == 'perm') *code = 1;
        *error = status_message(@"Create process tap", status); return NO;
    }
    UInt32 size = sizeof(capture->_format);
    status = get_property(capture->_tap, kAudioTapPropertyFormat, kAudioObjectPropertyScopeGlobal, &size, &capture->_format);
    if (status) { *error = status_message(@"Read tap format", status); return NO; }
    capture->_outputDevice = default_device(NO);
    NSString *outputUID = string_property(capture->_outputDevice, kAudioDevicePropertyDeviceUID);
    if (!outputUID) { *error = @"No default audio output is available."; return NO; }
    // Aggregate input streams are sub-device inputs followed by taps. Never
    // include a headset's microphone in a system/app-only recording.
    if (!input_buffer_count(capture->_outputDevice, &capture->_inputBufferOffset)) {
        *error = @"Could not determine the output device's input streams."; return NO;
    }
    NSDictionary *aggregate = @{
        @kAudioAggregateDeviceNameKey: @"Parlance capture",
        @kAudioAggregateDeviceUIDKey: NSUUID.UUID.UUIDString,
        @kAudioAggregateDeviceIsPrivateKey: @YES,
        @kAudioAggregateDeviceIsStackedKey: @NO,
        @kAudioAggregateDeviceMainSubDeviceKey: outputUID,
        @kAudioAggregateDeviceSubDeviceListKey: @[@{@kAudioSubDeviceUIDKey: outputUID}],
        @kAudioAggregateDeviceTapListKey: @[@{@kAudioSubTapUIDKey: description.UUID.UUIDString,
                                             @kAudioSubTapDriftCompensationKey: @YES}],
        @kAudioAggregateDeviceTapAutoStartKey: @YES
    };
    status = AudioHardwareCreateAggregateDevice((__bridge CFDictionaryRef)aggregate, &capture->_device);
    if (status) { *error = status_message(@"Create capture device", status); return NO; }
    capture->_aggregate = YES;
    return YES;
}

static BOOL start_screen_capture(ParlanceCapture *capture, pid_t pid, NSString **message, int *code) {
    dispatch_semaphore_t ready = dispatch_semaphore_create(0);
    __block SCShareableContent *content = nil;
    __block NSError *failure = nil;
    [SCShareableContent getShareableContentExcludingDesktopWindows:YES onScreenWindowsOnly:YES completionHandler:^(SCShareableContent *result, NSError *error) {
        content = result; failure = error; dispatch_semaphore_signal(ready);
    }];
    if (dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 60 * NSEC_PER_SEC))) {
        *message = @"Screen capture permission timed out. Allow Parlance in Privacy & Security > Screen Recording and retry.";
        return NO;
    }
    if (failure || content.displays.count == 0) {
        *code = failure.code == SCStreamErrorUserDeclined ? 1 : 2;
        *message = failure.localizedDescription ?: @"No display is available for audio capture.";
        return NO;
    }
    NSMutableArray *apps = [NSMutableArray new];
    for (SCRunningApplication *app in content.applications) {
        if (app.processID == (pid ? pid : getpid())) [apps addObject:app];
    }
    if (pid && apps.count == 0) { *code = 3; *message = @"The application is no longer available."; return NO; }
    SCContentFilter *filter = pid ? [[SCContentFilter alloc] initWithDisplay:content.displays.firstObject includingApplications:apps exceptingWindows:@[]] :
        [[SCContentFilter alloc] initWithDisplay:content.displays.firstObject excludingApplications:apps exceptingWindows:@[]];
    SCStreamConfiguration *config = [SCStreamConfiguration new];
    config.capturesAudio = YES;
    config.excludesCurrentProcessAudio = YES;
    config.sampleRate = 48000;
    config.channelCount = 2;
    config.width = 2; config.height = 2;
    config.minimumFrameInterval = CMTimeMake(1, 1);
    config.showsCursor = NO;
    capture->_format = (AudioStreamBasicDescription){.mSampleRate = 48000, .mChannelsPerFrame = 2};
    capture->_queue = dispatch_queue_create("app.parlance.audio", DISPATCH_QUEUE_SERIAL);
    capture->_stream = [[SCStream alloc] initWithFilter:filter configuration:config delegate:capture];
    if (![capture->_stream addStreamOutput:capture type:SCStreamOutputTypeAudio sampleHandlerQueue:capture->_queue error:&failure]) {
        *message = failure.localizedDescription ?: @"ScreenCaptureKit rejected its audio output."; return NO;
    }
    // Keep callback context alive even if startup times out; stop fences the audio queue.
    ready = dispatch_semaphore_create(0);
    [capture->_stream startCaptureWithCompletionHandler:^(NSError *error) { failure = error; dispatch_semaphore_signal(ready); }];
    if (dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC))) {
        *message = @"ScreenCaptureKit did not start in time."; return NO;
    }
    if (failure) { *code = failure.code == SCStreamErrorUserDeclined ? 1 : 2; *message = failure.localizedDescription; return NO; }
    return YES;
}

void *parlance_macos_start(int kind, const char *identifier, void *context, ParlancePush push,
                          uint32_t *rate, uint16_t *channels, int *code, char **error) {
    @autoreleasepool {
        ParlanceCapture *capture = [ParlanceCapture new];
        capture->_context = context; capture->_push = push;
        atomic_init(&capture->_active, true); atomic_init(&capture->_failure, 0);
        NSString *message = nil;
        *code = 2;
        @try {
            NSString *target = [NSString stringWithUTF8String:identifier];
            capture->_pid = kind == 1 ? target.intValue : 0;
            if (kind == 2) {
                AVAuthorizationStatus permission = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
                if (permission == AVAuthorizationStatusNotDetermined) {
                    dispatch_semaphore_t ready = dispatch_semaphore_create(0);
                    __block BOOL granted = NO;
                    [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL allowed) {
                        granted = allowed; dispatch_semaphore_signal(ready);
                    }];
                    if (dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 60 * NSEC_PER_SEC)) || !granted)
                        permission = AVAuthorizationStatusDenied;
                    else permission = AVAuthorizationStatusAuthorized;
                }
                if (permission != AVAuthorizationStatusAuthorized) {
                    *code = 1; message = @"Allow Parlance in System Settings > Privacy & Security > Microphone.";
                } else {
                    capture->_defaultInput = [target isEqualToString:@"default"];
                    if (capture->_defaultInput) capture->_device = default_device(YES);
                    else for (NSNumber *device in objects(kAudioHardwarePropertyDevices)) {
                        if ([string_property(device.unsignedIntValue, kAudioDevicePropertyDeviceUID) isEqualToString:target]) {
                            capture->_device = device.unsignedIntValue; break;
                        }
                    }
                    if (!capture->_device || !input_format(capture->_device, &capture->_format)) {
                        *code = 3; message = @"The microphone is no longer available.";
                    }
                }
            } else if (@available(macOS 14.2, *)) {
                start_tap(capture, capture->_pid, &message, code);
            } else {
                start_screen_capture(capture, kind == 1 ? target.intValue : 0, &message, code);
            }
            if (!message && !capture->_stream) {
                if (!float_format(&capture->_format)) message = @"The audio device does not provide 32-bit float PCM.";
                else {
                    OSStatus status = AudioDeviceCreateIOProcID(capture->_device, audio_callback, (__bridge void *)capture, &capture->_proc);
                    if (!status) status = AudioDeviceStart(capture->_device, capture->_proc);
                    if (status) {
                        if (status == kAudioDevicePermissionsError || status == 'perm') *code = 1;
                        message = status_message(@"Start audio capture", status);
                    }
                }
            }
        } @catch (NSException *exception) { message = exception.reason ?: @"Apple audio API failed."; }
        if (message) { [capture stop]; *error = strdup(message.UTF8String); return NULL; }
        *rate = (uint32_t)capture->_format.mSampleRate;
        *channels = (uint16_t)capture->_format.mChannelsPerFrame;
        return (void *)CFBridgingRetain(capture);
    }
}

int parlance_macos_status(void *handle) {
    ParlanceCapture *capture = (__bridge ParlanceCapture *)handle;
    int failure = atomic_load(&capture->_failure);
    if (failure) return failure;
    if (capture->_pid && kill(capture->_pid, 0) && errno == ESRCH) return 2;
    if (capture->_device) {
        UInt32 alive = 0, size = sizeof(alive);
        if (get_property(capture->_device, kAudioDevicePropertyDeviceIsAlive, kAudioObjectPropertyScopeGlobal, &size, &alive) || !alive) return 2;
        if (capture->_outputDevice && capture->_outputDevice != default_device(NO)) return 3;
        if (capture->_defaultInput && capture->_device != default_device(YES)) return 3;
        AudioStreamBasicDescription current = {0};
        if (capture->_tap) {
            size = sizeof(current);
            if (get_property(capture->_tap, kAudioTapPropertyFormat, kAudioObjectPropertyScopeGlobal, &size, &current)) return 2;
        } else if (!input_format(capture->_device, &current)) return 2;
        if (current.mSampleRate != capture->_format.mSampleRate || current.mChannelsPerFrame != capture->_format.mChannelsPerFrame) return 1;
    }
    return 0;
}

void parlance_macos_stop(void *handle) {
    @autoreleasepool {
        ParlanceCapture *capture = CFBridgingRelease(handle);
        [capture stop];
    }
}

void parlance_macos_free(char *value) { free(value); }
