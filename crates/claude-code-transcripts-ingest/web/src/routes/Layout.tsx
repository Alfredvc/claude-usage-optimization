import { NavLink, Outlet } from "react-router-dom";

export function Layout() {
  return (
    <>
      <div className="header">
        <div className="tab-nav">
          <NavLink
            to="/dashboard"
            className={({ isActive }) => `tab-btn ${isActive ? "active" : ""}`}
          >
            Dashboard
          </NavLink>
          <NavLink
            to="/transcripts"
            className={({ isActive }) => `tab-btn ${isActive ? "active" : ""}`}
          >
            Transcripts
          </NavLink>
        </div>
      </div>
      <Outlet />
    </>
  );
}
