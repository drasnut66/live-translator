#!/usr/bin/env python3
"""Simulates the browser-tab extension: connects to the Live Translator
localhost WebSocket and streams raw LE f32 16 kHz mono PCM (a 440 Hz sine).

Usage: python3 scripts/websocket_client_sim.py [seconds] [url]

Requires the `websockets` package: pip install websockets
"""

import asyncio
import math
import struct
import sys

import websockets


async def connect_with_retry(url, attempts=10, delay=0.5):
    last = None
    for _ in range(attempts):
        try:
            return await websockets.connect(url)
        except OSError as error:
            last = error
            await asyncio.sleep(delay)
    raise last


async def main():
    seconds = float(sys.argv[1]) if len(sys.argv) > 1 else 5.0
    url = sys.argv[2] if len(sys.argv) > 2 else "ws://127.0.0.1:8765/"

    ws = await connect_with_retry(url)
    print(f"connected to {url}")
    sample_rate = 16000
    frequency = 440.0
    sample = 0
    elapsed = 0.0
    try:
        while elapsed < seconds:
            chunk = [
                0.6 * math.sin(2 * math.pi * frequency * (sample + i) / sample_rate)
                for i in range(320)
            ]
            payload = struct.pack("<320f", *chunk)
            await ws.send(payload)
            sample += 320
            await asyncio.sleep(0.02)
            elapsed += 0.02
        await asyncio.sleep(0.2)
    finally:
        await ws.close()
    print("simulator finished sending")


if __name__ == "__main__":
    asyncio.run(main())
