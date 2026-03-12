import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import ShareTaskPage from "./share/ShareTaskPage";
import { isTauriRuntime } from "./platform/runtime";
import "./index.css";

const searchParams = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
const shareToken = !isTauriRuntime() ? searchParams?.get("share_token") : null;

ReactDOM.createRoot(document.getElementById("root")).render(
    <React.StrictMode>
        {shareToken ? <ShareTaskPage token={shareToken} /> : <App />}
    </React.StrictMode>,
);
