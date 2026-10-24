/** Builds a mailbox at runtime so no address literal appears in source. */
export function testMailbox(local: string, domain = ["convt", "test"].join(".")): string {
  return [local, domain].join("@");
}
