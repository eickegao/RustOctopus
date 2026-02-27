import { BrowserRouter, Routes, Route, NavLink } from "react-router-dom";
import Dashboard from "./views/Dashboard";
import ConfigView from "./views/Config";
import Channels from "./views/Channels";
import Cron from "./views/Cron";
const navItems = [
  { to: "/", label: "Dashboard" },
  { to: "/config", label: "Config" },
  { to: "/channels", label: "Channels" },
  { to: "/cron", label: "Cron" },
];

export default function App() {
  return (
    <BrowserRouter>
      <div className="flex h-screen">
        <nav className="w-56 shrink-0 border-r border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 flex flex-col">
          <div className="p-4 text-lg font-bold tracking-tight">
            RustOctopus
          </div>
          <div className="flex-1 px-2 space-y-1">
            {navItems.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) =>
                  `block px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                    isActive
                      ? "bg-indigo-50 dark:bg-indigo-900/30 text-indigo-700 dark:text-indigo-300"
                      : "text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700"
                  }`
                }
              >
                {item.label}
              </NavLink>
            ))}
          </div>
        </nav>

        <main className="flex-1 overflow-y-auto">
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/config" element={<ConfigView />} />
            <Route path="/channels" element={<Channels />} />
            <Route path="/cron" element={<Cron />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}
