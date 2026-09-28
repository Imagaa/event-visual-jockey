"""Builds app icons from EVJ-Logo.png (needs ffmpeg on PATH).

assets/evj.ico          multi-size exe / installer icon (16..256, PNG entries)
assets/evj-icon-64.rgba raw RGBA 64x64 for the window icon
assets/evj-logo-160x103.rgba raw RGBA for the UI top bar
"""
import pathlib
import struct
import subprocess

root = pathlib.Path(__file__).resolve().parent.parent
src = root / "EVJ-Logo.png"
out = root / "assets"
out.mkdir(exist_ok=True)


def ffmpeg(vf, dst, fmt):
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", str(src), "-vf", vf, *fmt, str(dst)], check=True)


def square(n):
    return (f"scale={n}:{n}:force_original_aspect_ratio=decrease:flags=lanczos,"
            f"pad={n}:{n}:(ow-iw)/2:(oh-ih)/2:color=0x00000000,format=rgba")


sizes = [16, 24, 32, 48, 64, 128, 256]
pngs = []
for n in sizes:
    p = out / f"icon-{n}.png"
    ffmpeg(square(n), p, ["-frames:v", "1"])
    pngs.append(p.read_bytes())
    p.unlink()

# ICO with PNG-compressed entries (Windows Vista+).
header = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
entries, blobs = b"", b""
for n, data in zip(sizes, pngs):
    entries += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
    blobs += data
(out / "evj.ico").write_bytes(header + entries + blobs)

ffmpeg(square(64), out / "evj-icon-64.rgba", ["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba"])
ffmpeg("scale=160:-1:flags=lanczos,format=rgba", out / "evj-logo.png", ["-frames:v", "1"])
ffmpeg("scale=160:103:flags=lanczos,format=rgba", out / "evj-logo-160x103.rgba", ["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba"])
print("icons written to", out)
