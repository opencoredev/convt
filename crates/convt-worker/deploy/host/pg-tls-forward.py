"""Loopback Postgres forwarder that verifies Railway's pinned certificate.

Railway's Postgres presents a leaf marked CA:TRUE, which rustls (and so sqlx)
refuses. This forwarder accepts plaintext on 127.0.0.1, performs Postgres's
SSLRequest upstream, and verifies the server against the pinned chain with
OpenSSL before relaying bytes. Usage: pg-tls-forward.py LISTEN_PORT HOST PORT CAFILE
"""
import asyncio
import ssl
import struct
import sys

listen_port, host, port, cafile = int(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4]
context = ssl.create_default_context(cafile=cafile)
# The certificate names only localhost and postgres.railway.internal.
SERVER_NAME = "localhost"


async def pipe(reader, writer):
    try:
        while data := await reader.read(65536):
            writer.write(data)
            await writer.drain()
    finally:
        writer.close()


async def handle(client_reader, client_writer):
    try:
        raw_reader, raw_writer = await asyncio.open_connection(host, port)
        raw_writer.write(struct.pack("!ii", 8, 80877103))
        await raw_writer.drain()
        if await raw_reader.readexactly(1) != b"S":
            raise ConnectionError("server refused TLS")
        await raw_writer.start_tls(context, server_hostname=SERVER_NAME)
    except Exception as error:
        print(f"pg-tls-forward: upstream failed: {error}", file=sys.stderr, flush=True)
        client_writer.close()
        return
    await asyncio.gather(
        pipe(client_reader, raw_writer), pipe(raw_reader, client_writer), return_exceptions=True
    )


async def main():
    server = await asyncio.start_server(handle, "127.0.0.1", listen_port)
    async with server:
        await server.serve_forever()


asyncio.run(main())
