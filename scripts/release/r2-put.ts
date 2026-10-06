/** Create-only S3 PUT. The server evaluates the precondition atomically. */
export async function createOnlyPut(url: string, body: Blob): Promise<void> {
  if (body.size > 5 * 1024 ** 3) throw Error("object exceeds the single-PUT limit");
  let response: Response;
  try {
    response = await fetch(url, {
      method: "PUT",
      headers: { "If-None-Match": "*" },
      body,
    });
  } catch {
    // A presigned URL is a temporary capability; never print it in errors.
    throw Error("R2 upload transport failed");
  }
  if (!response.ok) throw Error(`create-only upload failed (${response.status})`);
  await response.arrayBuffer();
}
