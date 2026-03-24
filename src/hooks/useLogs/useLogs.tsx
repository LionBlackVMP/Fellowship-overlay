import { useEffect } from "react";
import {
  chooseLogDirectory as chooseLogDirectoryApi,
  getDefaultLogPath,
  getOverlayState,
  listenOverlayState,
  setLogDirectory as setLogDirectoryApi,
  setOverlayEnabled as setOverlayEnabledApi,
} from "../../api";
import { useAppDispatch, useAppSelector } from "../../store/hooks";
import { applyServerUpdate, setError, setLogPath, setStatus } from "../../store/overlay";

const BOOTSTRAP_ATTEMPTS = 8;
const BOOTSTRAP_RETRY_DELAY_MS = 250;

export function useLogs() {
  const dispatch = useAppDispatch();
  const snapshot = useAppSelector((state) => state.overlay.snapshot);
  const logPath = useAppSelector((state) => state.overlay.logPath);
  const status = useAppSelector((state) => state.overlay.status);
  const error = useAppSelector((state) => state.overlay.error);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    dispatch(setStatus("loading"));
    dispatch(setError(null));

    const wait = (ms: number) =>
      new Promise((resolve) => {
        window.setTimeout(resolve, ms);
      });

    const connectListener = async () => {
      if (cancelled || unlisten) {
        return;
      }

      const nextUnlisten = await listenOverlayState((nextState) => {
        if (!cancelled) {
          dispatch(applyServerUpdate(nextState));
        }
      });
      if (cancelled) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
    };

    const connectLiveState = async () => {
      await connectListener();

      try {
        let initialState = null;
        let lastError: unknown = null;

        for (let attempt = 0; attempt < BOOTSTRAP_ATTEMPTS; attempt += 1) {
          try {
            initialState = await getOverlayState();
            break;
          } catch (nextError) {
            lastError = nextError;

            if (attempt < BOOTSTRAP_ATTEMPTS - 1) {
              await wait(BOOTSTRAP_RETRY_DELAY_MS);
            }
          }
        }

        if (initialState === null) {
          throw lastError instanceof Error ? lastError : new Error(String(lastError));
        }

        if (cancelled) {
          return;
        }
        dispatch(applyServerUpdate(initialState));
      } catch (nextError) {
        if (unlisten) {
          unlisten();
          unlisten = null;
        }
        if (!cancelled) {
          dispatch(setStatus("error"));
          dispatch(setError(nextError instanceof Error ? nextError.message : String(nextError)));
        }
      }
    };

    const bootstrap = async () => {
      try {
        const savedLogPath = await getDefaultLogPath();
        if (cancelled) {
          return;
        }

        dispatch(setLogPath(savedLogPath));

        if (!savedLogPath.trim()) {
          dispatch(setStatus("idle"));
          await connectListener();
          return;
        }

        await connectLiveState();
      } catch (nextError) {
        if (!cancelled) {
          dispatch(setStatus("error"));
          dispatch(setError(nextError instanceof Error ? nextError.message : String(nextError)));
        }
      }
    };

    void bootstrap();

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [dispatch]);

  return {
    snapshot,
    logPath,
    status,
    error,
    setOverlayEnabled: async (enabled: boolean) => {
      const nextState = await setOverlayEnabledApi(enabled);
      dispatch(applyServerUpdate(nextState));
    },
    setLogDirectory: async (path: string) => {
      const nextState = await setLogDirectoryApi(path);
      dispatch(applyServerUpdate(nextState));
    },
    chooseLogDirectory: async () => {
      const nextState = await chooseLogDirectoryApi();
      dispatch(applyServerUpdate(nextState));
    },
  };
}
