import { useCallback, useEffect, useRef, useState } from "react";

const VOLUME = 0.5;

/** osu!'s own preview clip for a set. */
export const previewUrl = (setId: number) => `https://b.ppy.sh/preview/${setId}.mp3`;

/** One preview at a time: starting another stops the first, and it stops itself when it ends. */
export function usePreview() {
  const audio = useRef<HTMLAudioElement | null>(null);
  const playing = useRef<number | null>(null);
  const [playingId, setPlayingId] = useState<number | null>(null);

  const stop = useCallback(() => {
    audio.current?.pause();
    audio.current = null;
    playing.current = null;
    setPlayingId(null);
  }, []);

  const toggle = useCallback(
    (setId: number, url?: string | null) => {
      if (playing.current === setId) {
        stop();
        return;
      }
      stop();
      const element = new Audio(url || previewUrl(setId));
      element.volume = VOLUME;
      const finish = () => {
        if (audio.current === element) stop();
      };
      element.onended = finish;
      element.onerror = finish;
      audio.current = element;
      playing.current = setId;
      setPlayingId(setId);
      element.play().catch(finish);
    },
    [stop],
  );

  useEffect(() => stop, [stop]);

  return { playingId, toggle, stop };
}
