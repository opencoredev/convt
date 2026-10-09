// Fails the build when an action in openapi/public.yaml targets nothing in the spec.
// Blume skips unmatched targets silently, so a spec refactor (an inline schema moving
// to components, say) would otherwise drop the overlay's prose without a warning.
// Targets are plain paths: `$`, `.name` and `['name']`. Wildcards and filters are
// refused because they can match nothing without being wrong.

const spec = await Bun.file(
  new URL("../../../crates/convt-server/openapi.json", import.meta.url),
).json();
const overlay = Bun.YAML.parse(
  await Bun.file(new URL("../openapi/public.yaml", import.meta.url)).text(),
) as { actions: { target: string }[] };

const segment = /\.([A-Za-z_$][\w$-]*)|\['([^']+)'\]/y;

function resolve(target: string): unknown {
  if (!target.startsWith("$")) throw new Error(`${target}: targets start with $`);
  let value: unknown = spec;
  segment.lastIndex = 1;
  while (segment.lastIndex < target.length) {
    const at = segment.lastIndex;
    const match = segment.exec(target);
    if (!match) throw new Error(`${target}: unsupported syntax at "${target.slice(at)}"`);
    value = (value as Record<string, unknown> | undefined)?.[match[1] ?? match[2]];
  }
  return value;
}

const unmatched = overlay.actions
  .map((a) => a.target)
  .filter((target) => resolve(target) === undefined);
if (unmatched.length) {
  throw new Error(`openapi/public.yaml targets nothing in the spec:\n  ${unmatched.join("\n  ")}`);
}
console.log(`public.yaml: ${overlay.actions.length} overlay targets match the spec`);
