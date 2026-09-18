import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// 桌面应用：禁用 WebView 默认右键菜单
document.addEventListener("contextmenu", (event) => {
  event.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
