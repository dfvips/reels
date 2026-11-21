export async function unwrap<T>(
  result: Promise<{ status: "ok"; data: T } | { status: "error"; error: string }>
): Promise<T> {
  const value = await result;
  if (value.status === "ok") return value.data;
  throw new Error(value.error);
}
