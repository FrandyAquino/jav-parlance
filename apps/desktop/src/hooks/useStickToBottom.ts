import { useCallback, useEffect, useRef, useState } from "react";

const THRESHOLD_PX = 48;

/**
 * Follows new content while the user is at the bottom; stops following once
 * they scroll up to read, and exposes `jumpToLatest` to resume.
 */
export function useStickToBottom<T extends HTMLElement>(contentKey: unknown) {
  const ref = useRef<T>(null);
  const [following, setFollowing] = useState(true);

  const onScroll = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    setFollowing(el.scrollHeight - el.scrollTop - el.clientHeight < THRESHOLD_PX);
  }, []);

  const jumpToLatest = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
    setFollowing(true);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (el && following) el.scrollTop = el.scrollHeight;
  }, [contentKey, following]);

  return { ref, following, onScroll, jumpToLatest };
}
