import { Link } from "react-router-dom";
import { GitCommit } from "lucide-react";
import { useCommits } from "../lib/hooks";
import { useTheme } from "../lib/theme";
import { MONO, fmtTime } from "../lib/format";
import { Badge } from "../components/Badge";
import { useProject } from "../lib/useProject";

function statusDot(
  status: "not-started" | "running" | "complete",
  C: ReturnType<typeof useTheme>["C"],
) {
  if (status === "complete") return C.green;
  if (status === "running") return C.amber;
  return C.textDim;
}

function statusBadge(
  status: "not-started" | "running" | "complete",
  C: ReturnType<typeof useTheme>["C"],
) {
  if (status === "complete")
    return { color: C.green, bg: C.green + "18", label: "complete" };
  if (status === "running")
    return { color: C.amber, bg: C.amber + "18", label: "running" };
  return { color: C.textDim, bg: C.surface3, label: "not started" };
}

export default function CommitsListPage() {
  const { C } = useTheme();
  const { commits, loading, error } = useCommits();
  const { current } = useProject();

  return (
    <div
      style={{
        flex: 1,
        overflowY: "auto",
        background: C.bg,
      }}
    >
      <div
        style={{
          maxWidth: 900,
          margin: "0 auto",
          padding: 16,
          display: "flex",
          flexDirection: "column",
          gap: 12,
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            border: `1px solid ${C.border}`,
            borderRadius: 6,
            background: C.surface,
            padding: "10px 12px",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <GitCommit size={14} color={C.accent} />
            <span
              style={{
                fontSize: 12,
                fontFamily: MONO,
                color: C.accent,
                fontWeight: 700,
                letterSpacing: 0.4,
                textTransform: "uppercase",
              }}
            >
              Commits
            </span>
          </div>
          <span style={{ fontSize: 10, color: C.textDim, fontFamily: MONO }}>
            {commits.length} total
          </span>
        </div>

        <div
          style={{
            border: `1px solid ${C.border}`,
            borderRadius: 6,
            overflow: "hidden",
            background: C.surface,
          }}
        >
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "16px 74px 1fr 130px 86px 78px 104px",
              gap: 8,
              padding: "6px 12px",
              fontSize: 8,
              fontWeight: 700,
              color: C.textDim,
              textTransform: "uppercase",
              letterSpacing: 0.8,
              borderBottom: `1px solid ${C.border}`,
              background: C.surface2,
            }}
          >
            <span />
            <span>Commit</span>
            <span>Message</span>
            <span>Author</span>
            <span>Time</span>
            <span style={{ textAlign: "right" }}>Measures</span>
            <span>Status</span>
          </div>

          {loading && (
            <div
              style={{
                padding: "18px 12px",
                fontSize: 11,
                color: C.textDim,
                fontFamily: MONO,
              }}
            >
              loading commits…
            </div>
          )}

          {error && !loading && (
            <div
              style={{
                padding: "18px 12px",
                fontSize: 11,
                color: C.red,
                fontFamily: MONO,
              }}
            >
              failed to load commits: {error}
            </div>
          )}

          {!loading && !error && commits.length === 0 && (
            <div
              style={{
                padding: "18px 12px",
                fontSize: 11,
                color: C.textDim,
                fontFamily: MONO,
              }}
            >
              no commits yet
            </div>
          )}

          {!loading &&
            !error &&
            commits.map((c) => {
              const badge = statusBadge(c.status, C);
              return (
                <Link
                  key={c.sha}
                  to={
                    current
                      ? `/project/${current.id}/commit/${c.shortSha}`
                      : "/"
                  }
                  style={{
                    display: "grid",
                    gridTemplateColumns: "16px 74px 1fr 130px 86px 78px 104px",
                    gap: 8,
                    padding: "7px 12px",
                    alignItems: "center",
                    textDecoration: "none",
                    color: C.text,
                    borderBottom: `1px solid ${C.border}`,
                  }}
                >
                  <span
                    style={{
                      width: 8,
                      height: 8,
                      borderRadius: 999,
                      background: statusDot(c.status, C),
                      boxShadow: `0 0 0 1px ${C.border}`,
                    }}
                  />
                  <span
                    style={{
                      fontFamily: MONO,
                      color: C.pink,
                      fontSize: 11,
                      fontWeight: 600,
                    }}
                  >
                    {c.shortSha}
                  </span>
                  <span
                    title={c.message}
                    style={{
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      fontSize: 12,
                    }}
                  >
                    {c.message}
                  </span>
                  <span
                    style={{
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      fontSize: 11,
                      color: C.cyan,
                      fontFamily: MONO,
                    }}
                  >
                    {c.author}
                  </span>
                  <span
                    style={{ fontSize: 10, color: C.textDim, fontFamily: MONO }}
                  >
                    {fmtTime(c.timestamp)}
                  </span>
                  <span style={{ display: "flex", justifyContent: "flex-end" }}>
                    <Badge color={C.textMid} bg={C.surface2}>
                      {c.measurements.length}
                    </Badge>
                  </span>
                  <span>
                    <Badge color={badge.color} bg={badge.bg}>
                      {badge.label}
                    </Badge>
                  </span>
                </Link>
              );
            })}
        </div>
      </div>
    </div>
  );
}
