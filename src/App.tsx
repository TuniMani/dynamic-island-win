import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { DynamicIsland } from "./components/DynamicIsland";
import { SettingsWindow } from "./components/SettingsWindow";

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
  return isSettings ? <SettingsWindow /> : <DynamicIsland />;
}

export default App;
