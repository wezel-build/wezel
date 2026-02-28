import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { useTheme } from "../lib/theme";
import { MONO, SANS } from "../lib/format";
import { useProject } from "../lib/useProject";
import { Workflow } from "lucide-react";

export default function NewProjectPage() {
  const { C } = useTheme();
  const navigate = useNavigate();
  const { projects, addProject } = useProject();

  const [name, setName] = useState("");
  const [upstream, setUpstream] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const canSubmit = name.trim() && upstream.trim() && !creating;

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setCreating(true);
    setError(null);
    try {
      await addProject(name.trim(), upstream.trim());
      navigate("/");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const empty = projects.length === 0;

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 24,
      }}
    >
      <div style={{ width: "100%", maxWidth: 400 }}>
        {empty && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              marginBottom: 24,
              justifyContent: "center",
            }}
          >
            <Workflow size={20} color={C.accent} strokeWidth={2.5} />
            <span
              style={{
                fontSize: 16,
                fontWeight: 800,
                color: C.accent,
                letterSpacing: -0.5,
                fontFamily: SANS,
              }}
            >
              Welcome to wezel
            </span>
          </div>
        )}

        <h2
          style={{
            fontSize: 14,
            fontFamily: MONO,
            fontWeight: 700,
            color: C.text,
            margin: 0,
            marginBottom: 4,
          }}
        >
          {empty ? "Create your first project" : "New project"}
        </h2>
        <p
          style={{
            fontSize: 11,
            fontFamily: MONO,
            color: C.textDim,
            margin: 0,
            marginBottom: 20,
          }}
        >
          Link a GitHub repository to start tracking builds.
        </p>

        <form onSubmit={handleSubmit}>
          <label
            style={{
              display: "block",
              fontSize: 10,
              fontFamily: MONO,
              fontWeight: 700,
              color: C.textDim,
              textTransform: "uppercase",
              letterSpacing: 0.8,
              marginBottom: 4,
            }}
          >
            Name
          </label>
          <input
            autoFocus
            placeholder="my-project"
            value={name}
            onChange={(e) => setName(e.target.value)}
            style={{
              width: "100%",
              padding: "8px 10px",
              fontSize: 13,
              fontFamily: MONO,
              background: C.surface,
              color: C.text,
              border: `1px solid ${C.border}`,
              borderRadius: 6,
              outline: "none",
              boxSizing: "border-box",
              marginBottom: 14,
            }}
          />

          <label
            style={{
              display: "block",
              fontSize: 10,
              fontFamily: MONO,
              fontWeight: 700,
              color: C.textDim,
              textTransform: "uppercase",
              letterSpacing: 0.8,
              marginBottom: 4,
            }}
          >
            GitHub repo URL
          </label>
          <input
            placeholder="https://github.com/org/repo"
            value={upstream}
            onChange={(e) => setUpstream(e.target.value)}
            style={{
              width: "100%",
              padding: "8px 10px",
              fontSize: 13,
              fontFamily: MONO,
              background: C.surface,
              color: C.text,
              border: `1px solid ${C.border}`,
              borderRadius: 6,
              outline: "none",
              boxSizing: "border-box",
              marginBottom: 20,
            }}
          />

          {error && (
            <div
              style={{
                fontSize: 11,
                fontFamily: MONO,
                color: C.red,
                marginBottom: 12,
              }}
            >
              {error}
            </div>
          )}

          <div
            style={{
              display: "flex",
              gap: 8,
              justifyContent: "flex-end",
            }}
          >
            {!empty && (
              <button
                type="button"
                onClick={() => navigate("/")}
                style={{
                  fontSize: 11,
                  fontFamily: MONO,
                  padding: "7px 16px",
                  background: "transparent",
                  color: C.textDim,
                  border: `1px solid ${C.border}`,
                  borderRadius: 6,
                  cursor: "pointer",
                }}
              >
                cancel
              </button>
            )}
            <button
              type="submit"
              disabled={!canSubmit}
              style={{
                fontSize: 11,
                fontFamily: MONO,
                fontWeight: 700,
                padding: "7px 20px",
                background: canSubmit ? C.accent : C.surface2,
                color: canSubmit ? C.bg : C.textDim,
                border: "none",
                borderRadius: 6,
                cursor: canSubmit ? "pointer" : "not-allowed",
              }}
            >
              {creating ? "creating…" : "create project"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
