import { Github } from "lucide-react";
import { authApi } from "../lib/api";
import { MONO, SANS } from "../lib/format";

export default function LoginPage() {
  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        background: "#0f0f0f",
        color: "#e8e4de",
        fontFamily: SANS,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 0,
      }}
    >
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 32,
          padding: "48px 40px",
          background: "#1a1a1a",
          border: "1px solid #2e2e2e",
          borderRadius: 12,
          minWidth: 320,
        }}
      >
        {/* Logo + name */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
          }}
        >
          <img src="/wezel.svg" width={28} height={28} alt="wezel" />
          <span
            style={{
              fontSize: 22,
              fontWeight: 800,
              color: "#e07b39",
              letterSpacing: -0.5,
              fontFamily: MONO,
            }}
          >
            wezel
          </span>
        </div>

        <div
          style={{
            textAlign: "center",
            display: "flex",
            flexDirection: "column",
            gap: 6,
          }}
        >
          <div
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "#e8e4de",
            }}
          >
            Sign in to continue
          </div>
          <div
            style={{
              fontSize: 12,
              color: "#666",
              fontFamily: MONO,
            }}
          >
            Authentication is required
          </div>
        </div>

        <a
          href={authApi.loginUrl}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "10px 24px",
            background: "#e8e4de",
            color: "#0f0f0f",
            borderRadius: 7,
            textDecoration: "none",
            fontSize: 13,
            fontWeight: 700,
            fontFamily: MONO,
            letterSpacing: 0.2,
            transition: "opacity 0.15s",
          }}
          onMouseEnter={(e) => (e.currentTarget.style.opacity = "0.85")}
          onMouseLeave={(e) => (e.currentTarget.style.opacity = "1")}
        >
          <Github size={16} />
          Sign in with GitHub
        </a>
      </div>
    </div>
  );
}