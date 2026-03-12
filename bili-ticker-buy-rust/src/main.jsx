import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import AdminTokenGatePage from "./AdminTokenGatePage";
import ShareTaskPage from "./share/ShareTaskPage";
import { ensureWebSession, onAdminAuthInvalid } from "./platform/apiClient";
import { isTauriRuntime } from "./platform/runtime";
import "./index.css";

function RootApp() {
    const [mode, setMode] = useState("loading");
    const searchParams = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
    const shareToken = !isTauriRuntime() ? searchParams?.get("share_token") : null;

    useEffect(() => {
        if (shareToken) {
            setMode("share");
            return () => {};
        }

        if (isTauriRuntime()) {
            setMode("app");
            return () => {};
        }

        let disposed = false;
        const bootstrap = async () => {
            try {
                await ensureWebSession();
                if (!disposed) {
                    setMode("app");
                }
            } catch (_) {
                if (!disposed) {
                    setMode("gate");
                }
            }
        };
        bootstrap();

        const off = onAdminAuthInvalid(() => {
            if (!disposed) {
                setMode("gate");
            }
        });

        return () => {
            disposed = true;
            off();
        };
    }, [shareToken]);

    if (mode === "share" && shareToken) {
        return <ShareTaskPage token={shareToken} />;
    }

    if (mode === "app") {
        return <App onAdminLogout={() => setMode("gate")} />;
    }

    if (mode === "gate") {
        return <AdminTokenGatePage onAuthenticated={() => setMode("app")} />;
    }

    return (
        <div className="min-h-screen bg-gray-950 text-white flex items-center justify-center">
            <div className="text-sm text-gray-400">正在检查后台访问权限...</div>
        </div>
    );
}

ReactDOM.createRoot(document.getElementById("root")).render(
    <React.StrictMode>
        <RootApp />
    </React.StrictMode>,
);
