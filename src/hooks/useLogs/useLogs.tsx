import { useEffect, useMemo } from "react";
import { getDefaultLogPath, pollOverlayState, setOverlayEnabled as setOverlayEnabledApi } from "../../api";
import {
  applyPollingSuccess,
  setError,
  setLogPath,
  setStatus,
} from "../../store/overlay";
import { useAppDispatch, useAppSelector } from "../../store/hooks";

export function useLogs() {
  const dispatch = useAppDispatch();
  const snapshot = useAppSelector((state) => state.overlay.snapshot);
  const logPath = useAppSelector((state) => state.overlay.logPath);
  const status = useAppSelector((state) => state.overlay.status);
  const error = useAppSelector((state) => state.overlay.error);
  const view = new URLSearchParams(window.location.search).get("view") ?? "settings";
  const isOverlayView = view === "overlay";
  const pollIntervalMs = 250;

  useEffect(() => {
    let cancelled = false;

    getDefaultLogPath()
      .then((path) => {
        if (!cancelled && path) {
          dispatch(setLogPath(path));
        }
      })
      .catch(() => {
        // Keep the fallback path when the backend command is unavailable.
      });

    return () => {
      cancelled = true;
    };
  }, [dispatch]);

  useEffect(() => {
    if (!logPath.trim()) {
      dispatch(setStatus("idle"));
      dispatch(setError("Specify the combat log path to start reading."));
      return;
    }

    let cancelled = false;
    dispatch(setStatus("loading"));
    dispatch(setError(null));

    const poll = async () => {
      try {
        const nextSnapshot = await pollOverlayState(logPath, isOverlayView);

        if (cancelled) {
          return;
        }

        dispatch(applyPollingSuccess(nextSnapshot));
      } catch (err) {
        if (cancelled) {
          return;
        }

        dispatch(setStatus("error"));
        dispatch(setError(err instanceof Error ? err.message : String(err)));
      }
    };

    void poll();

    if (!isOverlayView) {
      return () => {
        cancelled = true;
      };
    }

    const interval = window.setInterval(() => {
      void poll();
    }, pollIntervalMs);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [dispatch, isOverlayView, logPath, pollIntervalMs]);

  return useMemo(
    () => ({
      snapshot,
      logPath,
      setLogPath: (value: string) => dispatch(setLogPath(value)),
      setOverlayEnabled: async (enabled: boolean) => {
        const nextSnapshot = await setOverlayEnabledApi(enabled);
        dispatch(applyPollingSuccess(nextSnapshot));
      },
      status,
      error,
    }),
    [dispatch, error, logPath, snapshot, status],
  );
}
