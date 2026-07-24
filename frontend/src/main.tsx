import React from "react";
import ReactDOM from "react-dom/client";
import "reactflow/dist/style.css";
import "./styles.css";
import { EditorPage } from "./pages/EditorPage";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <EditorPage />
  </React.StrictMode>,
);
