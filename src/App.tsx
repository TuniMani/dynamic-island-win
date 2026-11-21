import { lazy, Suspense } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { DynamicIsland } from "./components/DynamicIsland";

const SettingsWindow = lazy(() =>
  import("./components/SettingsWindow").then((m) => ({ default: m.SettingsWindow })),
);

function detectSettingsWindow(): boolean {
  try {
    if (getCurrentWebviewWindow().label === "settings") {
      return true;
    }
  } catch {
    // Fallback to URL query check when outside Tauri runtime
  }
  return (
    typeof window !== "undefined" &&
    window.location.search.includes("window=settings")
  );
}

export function App() {
  const isSettings = detectSettingsWindow();
  if (isSettings) {
    return (
      <Suspense fallback={<div className="h-full w-full bg-[#0b0d13]" />}>
        <SettingsWindow />
      </Suspense>
    );
  }
  return <DynamicIsland />;
}

export default App;
