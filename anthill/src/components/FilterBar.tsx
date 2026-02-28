import { Search, X } from "lucide-react";
import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";
import { USERS } from "../lib/data";

export function FilterBar({
  search,
  onSearch,
  userFilter,
  onUserFilter,
  profileFilter,
  onProfileFilter,
}: {
  search: string;
  onSearch: (v: string) => void;
  userFilter: string[];
  onUserFilter: (v: string[]) => void;
  profileFilter: string | null;
  onProfileFilter: (v: string | null) => void;
}) {
  const { C } = useTheme();
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 0",
        fontSize: 11,
        flexWrap: "wrap",
      }}
    >
      {/* Search */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 4,
          background: C.surface2,
          border: `1px solid ${C.border}`,
          borderRadius: 4,
          padding: "3px 8px",
          minWidth: 180,
        }}
      >
        <Search size={12} color={C.textDim} />
        <input
          id="scenario-search"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder="filter commands…"
          style={{
            background: "transparent",
            border: "none",
            outline: "none",
            color: C.text,
            fontSize: 11,
            fontFamily: MONO,
            width: "100%",
          }}
        />
        {search && (
          <button
            onClick={() => onSearch("")}
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              padding: 0,
              display: "flex",
            }}
          >
            <X size={11} color={C.textDim} />
          </button>
        )}
      </div>

      {/* User filter */}
      <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
        <span
          style={{
            color: C.textDim,
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: 0.5,
          }}
        >
          USER
        </span>
        {USERS.map((u) => (
          <button
            key={u}
            onClick={() =>
              onUserFilter(
                userFilter.includes(u)
                  ? userFilter.filter((x) => x !== u)
                  : [...userFilter, u],
              )
            }
            style={{
              background: userFilter.includes(u)
                ? C.accent + "22"
                : "transparent",
              border: `1px solid ${userFilter.includes(u) ? C.accent : C.border}`,
              borderRadius: 3,
              padding: "2px 7px",
              cursor: "pointer",
              color: userFilter.includes(u) ? C.accent : C.textMid,
              fontSize: 10,
              fontFamily: MONO,
            }}
          >
            {u}
          </button>
        ))}
      </div>

      {/* Profile filter */}
      <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
        <span
          style={{
            color: C.textDim,
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: 0.5,
          }}
        >
          PROFILE
        </span>
        {(["dev", "release"] as const).map((p) => (
          <button
            key={p}
            onClick={() => onProfileFilter(profileFilter === p ? null : p)}
            style={{
              background: profileFilter === p ? C.accent + "22" : "transparent",
              border: `1px solid ${profileFilter === p ? C.accent : C.border}`,
              borderRadius: 3,
              padding: "2px 7px",
              cursor: "pointer",
              color: profileFilter === p ? C.accent : C.textMid,
              fontSize: 10,
              fontFamily: MONO,
              textTransform: "uppercase",
            }}
          >
            {p}
          </button>
        ))}
      </div>
    </div>
  );
}
