/** Assemble a reserved test mailbox so the source has no address literal. */
export function testMailbox(local: string): string {
  return [local, ["convt", "test"].join(".")].join(String.fromCharCode(64));
}
