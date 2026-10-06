// R2 S3 data transfer uses atomic create-only PUTs. Account and bucket
// management use the cf CLI. Large objects use the S3 endpoint, not REST.
import { readdirSync, statSync } from "node:fs";
import { resolve, basename } from "node:path";
import { createOnlyPut } from "./r2-put";
const dir = process.argv[2];
const bucket = process.env.CONVT_R2_BUCKET;
const account = process.env.CONVT_R2_ACCOUNT_ID;
if (
  !dir ||
  !bucket ||
  !account ||
  !process.env.AWS_ACCESS_KEY_ID ||
  !process.env.AWS_SECRET_ACCESS_KEY
)
  throw Error("set R2 bucket/account and scoped S3 access key credentials");
const client = new Bun.S3Client({
  bucket,
  endpoint: `https://${account}.r2.cloudflarestorage.com`,
  region: "auto",
  accessKeyId: process.env.AWS_ACCESS_KEY_ID,
  secretAccessKey: process.env.AWS_SECRET_ACCESS_KEY,
});
// Never expose keys. Immutable versions are uploaded before a publisher moves
// the stable manifest pointer. This command does not move that pointer.
for (const name of readdirSync(dir).sort()) {
  const path = resolve(dir, name);
  if (!statSync(path).isFile()) continue;
  const object = client.file(`${basename(dir)}/${name}`);
  await createOnlyPut(object.presign({ method: "PUT", expiresIn: 3600 }), Bun.file(path));
  console.log(`uploaded ${basename(dir)}/${name}`);
}
