import { Route, Routes } from "react-router-dom";
import { ShowingsPage } from "./pages/ShowingsPage";
import { ImpressumPage } from "./pages/ImpressumPage";

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<ShowingsPage />} />
      <Route path="/impressum" element={<ImpressumPage />} />
    </Routes>
  );
}
