import { Route, Routes, useSearchParams } from "react-router-dom";
import { AuthProvider } from "./hooks/useAuth";
import { ShowingsPage } from "./pages/ShowingsPage";
import { ImpressumPage } from "./pages/ImpressumPage";
import { LoginConfirmedPage } from "./pages/LoginConfirmedPage";
import { InvalidLinkPage } from "./pages/InvalidLinkPage";

export default function App() {
  const [searchParams] = useSearchParams();
  const confirmed = searchParams.get("login") === "confirmed";
  const invalid = searchParams.get("error") === "invalid_token";
  return (
    <AuthProvider>
      {invalid ? (
        <InvalidLinkPage />
      ) : confirmed ? (
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
