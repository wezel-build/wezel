/**
 * Detect which build toolchain an observation name corresponds to.
 * Returns a toolchain identifier (e.g. "cargo") or null if unknown.
 */
export function detectToolchain(observationName: string): string | null {
  const name = observationName.toLowerCase();
  const cargoPatterns = [
    "cargo build",
    "cargo check",
    "cargo test",
    "cargo run",
    "cargo clippy",
  ];
  for (const pattern of cargoPatterns) {
    if (name.includes(pattern.toLowerCase())) return "cargo";
  }
  if (name.startsWith("cargo")) return "cargo";
  return null;
}
