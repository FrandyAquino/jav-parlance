/** Returns a copy of `record` without `keys`, leaving the input untouched. */
export function without<K extends PropertyKey, V>(record: Record<K, V>, keys: Iterable<K>): Record<K, V> {
  const excluded = new Set<PropertyKey>(Array.from(keys, String));
  return Object.fromEntries(Object.entries(record).filter(([key]) => !excluded.has(key))) as Record<K, V>;
}
