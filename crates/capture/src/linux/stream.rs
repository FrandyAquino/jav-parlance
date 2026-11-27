use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use parlance_audio::StreamFormat;
use parlance_audio::ring::AudioSink;

use super::Target;
use crate::{CaptureError, CaptureEvent, CaptureEventHandler, CaptureStream};

const FORMAT: StreamFormat = StreamFormat { sample_rate: 48_000, channels: 2 };
const RECONNECT_DELAY: Duration = Duration::from_millis(500);
const MAX_RECONNECTS: u32 = 40;
/// 10 ms of interleaved f32 stereo.
const READ_BYTES: usize = 48_000 / 100 * 2 * 4;

/// Runs `parec` and copies its raw f32 output into the ring buffer. The
/// child process does the real-time work; this thread only converts bytes.
#[derive(Debug)]
pub struct ParecStream {
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    thread: Option<JoinHandle<()>>,
}

fn spawn_parec(target: &Target) -> std::io::Result<Child> {
    Command::new("parec")
        .args(["--raw", "--format=float32le", "--rate=48000", "--channels=2", "--latency-msec=20"])
        .args(["--client-name=Parlance", "--stream-name=Meeting capture"])
        .args(target.parec_args())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

impl ParecStream {
    pub fn spawn(target: Target, mut sink: AudioSink, on_event: CaptureEventHandler) -> Result<Self, CaptureError> {
        let first = spawn_parec(&target)
            .map_err(|e| CaptureError::Unsupported(format!("parec not found ({e}); install pulseaudio-utils")))?;
        let stop = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(Some(first)));
        let (stop_flag, child_slot) = (Arc::clone(&stop), Arc::clone(&child));

        let thread = std::thread::Builder::new()
            .name("parlance-parec".into())
            .spawn(move || {
                let mut attempts = 0;
                loop {
                    let stdout = child_slot
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .as_mut()
                        .and_then(|child| child.stdout.take());
                    if let Some(stdout) = stdout {
                        pump(stdout, &mut sink, &stop_flag);
                    }
                    reap_child(&child_slot);
                    if stop_flag.load(Ordering::Acquire) || !sink.is_consumer_alive() {
                        return;
                    }
                    if !target.is_reconnectable() || attempts >= MAX_RECONNECTS {
                        on_event(CaptureEvent::Failed { reason: "the audio stream ended".into() });
                        return;
                    }
                    on_event(CaptureEvent::DeviceLost { reason: "parec exited".into() });
                    attempts += 1;
                    std::thread::sleep(RECONNECT_DELAY);
                    // Publish under the same lock Drop uses, so cancellation cannot miss a new child.
                    let restarted = {
                        let mut slot = child_slot.lock().unwrap_or_else(|p| p.into_inner());
                        if stop_flag.load(Ordering::Acquire) || !sink.is_consumer_alive() {
                            return;
                        }
                        match spawn_parec(&target) {
                            Ok(next) => {
                                *slot = Some(next);
                                true
                            }
                            Err(error) => {
                                tracing::warn!(%error, "parec restart failed");
                                false
                            }
                        }
                    };
                    if restarted {
                        on_event(CaptureEvent::Reconnected);
                    }
                }
            })
            .map_err(|e| {
                reap_child(&child);
                CaptureError::Backend(e.to_string())
            })?;
        Ok(Self { stop, child, thread: Some(thread) })
    }
}

fn reap_child(slot: &Mutex<Option<Child>>) {
    let current = slot.lock().unwrap_or_else(|p| p.into_inner()).take();
    if let Some(mut child) = current {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn pump(mut stdout: impl Read, sink: &mut AudioSink, stop: &AtomicBool) {
    let mut bytes = vec![0u8; READ_BYTES];
    let mut pending = Vec::with_capacity(READ_BYTES + 4);
    let mut samples = Vec::with_capacity(READ_BYTES / 4);
    while !stop.load(Ordering::Acquire) {
        let read = match stdout.read(&mut bytes) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        pending.extend_from_slice(&bytes[..read]);
        let whole = pending.len() / 4 * 4;
        samples.clear();
        samples.extend(pending[..whole].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)));
        pending.drain(..whole);
        sink.push(&samples);
    }
}

impl CaptureStream for ParecStream {
    fn format(&self) -> StreamFormat {
        FORMAT
    }
}

impl Drop for ParecStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        reap_child(&self.child);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Instant;

    const DEADLINE: Duration = Duration::from_secs(10);

    // A separate test process supplies PATH to the production spawn entry point,
    // without mutating the environment of concurrently running Rust tests.
    fn run_lifecycle(scenario: &str) {
        let directory = tempfile::tempdir().unwrap();
        let parec = directory.path().join("parec");
        std::fs::write(
            &parec,
            "#!/bin/sh\nexec \"$PARLANCE_TEST_EXE\" --exact linux::stream::tests::fake_parec --ignored --nocapture\n",
        )
        .unwrap();
        std::fs::set_permissions(&parec, std::fs::Permissions::from_mode(0o700)).unwrap();
        let executable = std::env::current_exe().unwrap();
        let mut driver = Command::new(&executable)
            .args(["--exact", "linux::stream::tests::lifecycle_driver", "--ignored", "--nocapture"])
            .env("PATH", directory.path())
            .env("PARLANCE_TEST_EXE", &executable)
            .env("PARLANCE_TEST_SOCKET", directory.path().join("control.sock"))
            .env("PARLANCE_TEST_SCENARIO", scenario)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = driver.try_wait().unwrap() {
                assert!(status.success(), "lifecycle scenario {scenario} failed");
                break;
            }
            if Instant::now() >= deadline {
                let _ = driver.kill();
                let _ = driver.wait();
                panic!("lifecycle scenario {scenario} did not finish");
            }
            std::thread::yield_now();
        }
    }

    #[test]
    fn reconnect_reaps_each_previous_child() {
        run_lifecycle("reconnect");
    }

    #[test]
    fn drop_during_reconnect_leaves_no_child() {
        run_lifecycle("drop-reconnect");
    }

    #[test]
    fn normal_drop_terminates_and_reaps_child() {
        run_lifecycle("normal-drop");
    }

    #[test]
    #[ignore = "subprocess fixture invoked only by lifecycle tests"]
    fn fake_parec() {
        let Ok(path) = std::env::var("PARLANCE_TEST_SOCKET") else { return };
        let mut control = UnixStream::connect(path).unwrap();
        writeln!(control, "{}", std::process::id()).unwrap();
        let mut command = [0];
        let _ = control.read_exact(&mut command);
        std::process::exit(0);
    }

    fn accept_child(listener: &UnixListener) -> (u32, UnixStream) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            match listener.accept() {
                Ok((control, _)) => {
                    control.set_read_timeout(Some(DEADLINE)).unwrap();
                    let mut pid = String::new();
                    BufReader::new(&control).read_line(&mut pid).unwrap();
                    return (pid.trim().parse().unwrap(), control);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "parec never connected");
                    std::thread::yield_now();
                }
                Err(error) => panic!("parec control socket failed: {error}"),
            }
        }
    }

    fn assert_reaped(pid: u32) {
        assert!(!Path::new(&format!("/proc/{pid}")).exists(), "parec {pid} still exists (running or zombie)");
    }

    #[test]
    #[ignore = "isolated environment for production ParecStream entry point"]
    fn lifecycle_driver() {
        let Ok(scenario) = std::env::var("PARLANCE_TEST_SCENARIO") else { return };
        let listener = UnixListener::bind(std::env::var("PARLANCE_TEST_SOCKET").unwrap()).unwrap();
        listener.set_nonblocking(true).unwrap();
        let (events, received) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let gate = Mutex::new(gate);
        let pause_reconnect = scenario == "drop-reconnect";
        let (sink, _source) = parlance_audio::ring::channel(1024);
        let stream = ParecStream::spawn(
            Target::Monitor(None),
            sink,
            Arc::new(move |event| {
                let lost = matches!(event, CaptureEvent::DeviceLost { .. });
                events.send(event).unwrap();
                if lost && pause_reconnect {
                    gate.lock().unwrap().recv_timeout(DEADLINE).unwrap();
                }
            }),
        )
        .unwrap();
        let (first_pid, mut first) = accept_child(&listener);
        assert_eq!(stream.format(), FORMAT);
        if scenario == "normal-drop" {
            drop(stream);
            assert_reaped(first_pid);
            return;
        }

        first.write_all(b"exit").unwrap();
        assert!(matches!(received.recv_timeout(DEADLINE).unwrap(), CaptureEvent::DeviceLost { .. }));
        if scenario == "drop-reconnect" {
            let stop = Arc::clone(&stream.stop);
            let children = Arc::clone(&stream.child);
            let dropping = std::thread::spawn(move || drop(stream));
            let deadline = Instant::now() + DEADLINE;
            while !stop.load(Ordering::Acquire) || children.lock().unwrap().is_some() {
                assert!(Instant::now() < deadline, "Drop did not reach cleanup");
                std::thread::yield_now();
            }
            release.send(()).unwrap();
            dropping.join().unwrap();
            // Clean up a regressed late child before reporting the failure.
            let late = children.lock().unwrap().take();
            let late_pid = late.map(|mut child| {
                let pid = child.id();
                let _ = child.kill();
                let _ = child.wait();
                pid
            });
            assert!(late_pid.is_none(), "Drop left a replacement child behind: {late_pid:?}");
            assert_reaped(first_pid);
        } else {
            assert!(matches!(received.recv_timeout(DEADLINE).unwrap(), CaptureEvent::Reconnected));
            let (second_pid, mut second) = accept_child(&listener);
            let first_reaped = !Path::new(&format!("/proc/{first_pid}")).exists();
            second.write_all(b"exit").unwrap();
            assert!(matches!(received.recv_timeout(DEADLINE).unwrap(), CaptureEvent::DeviceLost { .. }));
            assert!(matches!(received.recv_timeout(DEADLINE).unwrap(), CaptureEvent::Reconnected));
            let (third_pid, _third) = accept_child(&listener);
            drop(stream);
            assert!(first_reaped, "the first child was replaced without being reaped");
            assert_reaped(second_pid);
            assert_reaped(third_pid);
        }
    }
}
