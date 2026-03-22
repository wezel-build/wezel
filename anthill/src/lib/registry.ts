import type {
  RegistryAdapter,
  RegistryTemplate,
  RegistryUiField,
} from "./data";
import type { CrateTopo } from "./data";

/**
 * The registry URL can be any valid URI:
 *   - https://wezel.dev/registry
 *   - http://localhost:3001/registry
 *   - file:///Users/alice/my-registry   (browser-restricted; works in CLI contexts)
 *
 * Set via VITE_REGISTRY_URL at build time, or leave unset to use the
 * bundled default (wezel.dev).
 */
const REGISTRY_URL =
  (import.meta.env.VITE_REGISTRY_URL as string | undefined)?.replace(
    /\/$/,
    "",
  ) ?? "https://wezel.dev/registry";

/**
 * Fetch an adapter definition for the given toolchain from the configured
 * registry URI.  Returns `null` if the adapter is not found or the fetch
 * fails.
 *
 * The registry is expected to serve adapters at:
 *   `{REGISTRY_URL}/adapters/{toolchain}.json`
 */
export async function fetchAdapter(
  toolchain: string,
): Promise<RegistryAdapter | null> {
  const url = `${REGISTRY_URL}/adapters/${toolchain}.json`;
  try {
    const res = await fetch(url);
    if (!res.ok) return null;
    return res.json() as Promise<RegistryAdapter>;
  } catch {
    return null;
  }
}

/**
 * Generate a benchmark.toml string from a template and field values.
 */
export function generateBenchmarkToml(
  benchmarkName: string,
  template: RegistryTemplate,
  values: Record<string, string>,
  _graph: CrateTopo[],
): string {
  const lines: string[] = [
    `name = "${benchmarkName}"`,
    `description = "${template.description}"`,
    "",
  ];

  for (const step of template.steps) {
    lines.push(`[[steps]]`);
    lines.push(`name = "${step.name}"`);
    lines.push(`forager = "${step.tool}"`);

    const inputs: Record<string, unknown> = { ...step.inputs };

    // Inject UI field values into inputs
    for (const field of (template.uiSchema?.fields ??
      []) as RegistryUiField[]) {
      const val = values[field.id];
      if (!val) continue;
      inputs[field.id] = val;
    }

    if (Object.keys(inputs).length > 0) {
      lines.push(`[steps.inputs]`);
      for (const [k, v] of Object.entries(inputs)) {
        if (typeof v === "object" && v !== null) {
          // Inline table for nested objects (e.g. env)
          const inner = Object.entries(v as Record<string, string>)
            .map(([ik, iv]) => `${ik} = ${JSON.stringify(iv)}`)
            .join(", ");
          lines.push(`${k} = { ${inner} }`);
        } else {
          lines.push(`${k} = ${JSON.stringify(v)}`);
        }
      }
    }
    lines.push("");
  }

  return lines.join("\n");
}

/** Sort graph nodes by heat (dirtiest first) for use in crate-picker. */
export function sortedCrateNames(
  graph: CrateTopo[],
  heat: Record<string, number>,
): string[] {
  return graph
    .filter((c) => !c.external)
    .map((c) => c.name)
    .sort((a, b) => (heat[b] ?? 0) - (heat[a] ?? 0));
}
