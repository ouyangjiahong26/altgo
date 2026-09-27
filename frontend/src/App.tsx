import { useState } from "react";
import { HashRouter, Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import Home from "./pages/Home";
import Settings from "./pages/Settings";
import Onboarding from "./components/Onboarding";
import { isOnboarded } from "./onboarding";

export default function App() {
  const [showOnboarding, setShowOnboarding] = useState(() => !isOnboarded());

  // 首次安装向导是整窗流程，不套主窗导航；完成后才进入正常两页界面。
  if (showOnboarding) {
    return <Onboarding onDone={() => setShowOnboarding(false)} />;
  }

  return (
    <HashRouter>
      <Layout>
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/settings" element={<Settings />} />
        </Routes>
      </Layout>
    </HashRouter>
  );
}
