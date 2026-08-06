import { Route, Routes } from "react-router-dom";
import { AuthProvider } from "./hooks/useAuth";
import { ShowingsPage } from "./pages/ShowingsPage";
import { ImpressumPage } from "./pages/ImpressumPage";

export default function App() {
  return (
    <AuthProvider>
      <Routes>
        <Route path="/" element={<ShowingsPage />} />
        <Route path="/impressum" element={<ImpressumPage />} />
      </Routes>
    </AuthProvider>
  );
}
