import { Route, Routes, useSearchParams } from "react-router-dom";
import { AuthProvider } from "./hooks/useAuth";
import { ShowingsPage } from "./pages/ShowingsPage";
import { ImpressumPage } from "./pages/ImpressumPage";
import { LoginConfirmedPage } from "./pages/LoginConfirmedPage";

export default function App() {
  const [searchParams] = useSearchParams();
  const confirmed = searchParams.get("login") === "confirmed";
  return (
    <AuthProvider>
      {confirmed ? (
        <LoginConfirmedPage />
      ) : (
        <Routes>
          <Route path="/" element={<ShowingsPage />} />
          <Route path="/impressum" element={<ImpressumPage />} />
        </Routes>
      )}
    </AuthProvider>
  );
}
