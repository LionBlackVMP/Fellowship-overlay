import { configureStore } from "@reduxjs/toolkit";
import { overlayReducer } from "./overlay";

export const store = configureStore({
  reducer: {
    overlay: overlayReducer,
  },
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
