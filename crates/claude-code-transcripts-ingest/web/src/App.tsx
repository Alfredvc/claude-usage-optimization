import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { Layout } from "./routes/Layout";
import { DashboardPage } from "./routes/DashboardPage";
import { SessionListPage } from "./routes/SessionListPage";
import { TranscriptPage } from "./routes/TranscriptPage";
import "./routes/SessionListPage.css";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Navigate to="/dashboard" replace />} />
          <Route path="/dashboard" element={<DashboardPage />} />
          <Route path="/transcripts" element={<SessionListPage />} />
          <Route path="/transcripts/:id" element={<TranscriptPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
