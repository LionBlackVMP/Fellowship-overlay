import React from "react";
import ReactDOM from "react-dom/client";
import { Provider } from "react-redux";
import App from "./App";
import { store } from "./store/store";

type RootErrorBoundaryState = {
  error: string | null;
};

class RootErrorBoundary extends React.Component<React.PropsWithChildren, RootErrorBoundaryState> {
  public state: RootErrorBoundaryState = {
    error: null,
  };

  static getDerivedStateFromError(error: unknown): RootErrorBoundaryState {
    return {
      error: error instanceof Error ? error.message : String(error),
    };
  }

  override componentDidCatch(error: unknown) {
    console.error("Root render failed:", error);
  }

  override render() {
    if (this.state.error) {
      return (
        <div
          style={{
            minHeight: "100vh",
            padding: "16px",
            background: "#101722",
            color: "#f5e7bf",
            fontFamily: "Segoe UI, sans-serif",
          }}
        >
          <strong>Frontend crashed:</strong> {this.state.error}
        </div>
      );
    }

    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <RootErrorBoundary>
    <Provider store={store}>
      <App />
    </Provider>
  </RootErrorBoundary>,
);
