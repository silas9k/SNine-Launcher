import { initializeUiStorage } from "./lib/uiStorage";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/index.css";

performance.mark("s9lab.app.start");

void initializeUiStorage()
  .catch((error) => {
    console.warn("[SNine Launcher] Native UI preference store could not be loaded", error);
  })
  .finally(() => {
    ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
  });
