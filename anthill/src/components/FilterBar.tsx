import { Search, X } from "lucide-react";
import { C, alpha } from "../lib/colors";
import { useUsers } from "../lib/hooks";

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
  const { users } = useUsers();
  return (
    <div className="flex items-center gap-[8px] py-[6px] text-[11px] flex-wrap">
      {/* Search */}
      <div
        className="flex items-center gap-[4px] rounded px-[8px] py-[3px] min-w-[180px] border border-[var(--c-border)]"
        style={{ background: C.surface2 }}
      >
        <Search size={12} color={C.textDim as string} />
        <input
          id="observation-search"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder="filter commands…"
          className="bg-transparent border-none outline-none text-[11px] font-mono w-full text-fg"
        />
        {search && (
          <button
            onClick={() => onSearch("")}
            className="bg-transparent border-none cursor-pointer p-0 flex"
          >
            <X size={11} color={C.textDim as string} />
          </button>
        )}
      </div>

      {/* User filter */}
      <div className="flex items-center gap-[4px]">
        <span
          className="text-[10px] font-semibold tracking-[0.5px]"
          style={{ color: C.textDim }}
        >
          USER
        </span>
        {users.map((u) => (
          <button
            key={u}
            onClick={() =>
              onUserFilter(
                userFilter.includes(u)
                  ? userFilter.filter((x) => x !== u)
                  : [...userFilter, u],
              )
            }
            className="rounded-[3px] py-[4px] px-[7px] cursor-pointer text-[10px] font-mono border"
            style={{
              background: userFilter.includes(u)
                ? alpha(C.accent, 13)
                : "transparent",
              borderColor: userFilter.includes(u) ? C.accent : C.border,
              color: userFilter.includes(u) ? C.accent : C.textMid,
            }}
          >
            {u}
          </button>
        ))}
      </div>

      {/* Profile filter */}
      <div className="flex items-center gap-[4px]">
        <span
          className="text-[10px] font-semibold tracking-[0.5px]"
          style={{ color: C.textDim }}
        >
          PROFILE
        </span>
        {(["dev", "release"] as const).map((p) => (
          <button
            key={p}
            onClick={() => onProfileFilter(profileFilter === p ? null : p)}
            className="rounded-[3px] py-[4px] px-[7px] cursor-pointer text-[10px] font-mono uppercase border"
            style={{
              background:
                profileFilter === p ? alpha(C.accent, 13) : "transparent",
              borderColor: profileFilter === p ? C.accent : C.border,
              color: profileFilter === p ? C.accent : C.textMid,
            }}
          >
            {p}
          </button>
        ))}
      </div>
    </div>
  );
}
