import pg from "pg";

/** Connects on the first query, so requests that never touch the database never connect. */
export class LazyClient extends pg.Client {
  private connecting: Promise<unknown> | null = null;

  // pg queues queries sent before the connection is up, once connect() has started.
  override query(...args: unknown[]): never {
    this.connecting ??= this.connect();
    return (super.query as (...a: unknown[]) => never)(...args);
  }

  async close() {
    if (this.connecting) await this.connecting.then(() => this.end()).catch(() => {});
  }
}
