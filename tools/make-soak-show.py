"""Builds a soak-test show from a media folder: `python tools/make-soak-show.py <media-dir> <out.vjproj> [deck.json ...]`.

Videos on layers 1 and 3, pictures on layer 2, sound on layer 4, presentations on layer 5.
Run it with `evj --soak 4 <out.vjproj>`.
"""
import json
import os
import sys

VIDEO = {'.mp4', '.mov', '.m4v', '.mkv', '.avi', '.wmv', '.webm'}
IMAGE = {'.jpg', '.jpeg', '.png', '.bmp', '.gif', '.tif', '.tiff', '.webp'}
AUDIO = {'.mp3', '.wav', '.m4a', '.aac', '.flac', '.wma'}

media, out, decks = sys.argv[1], sys.argv[2], sys.argv[3:]
files = sorted(os.path.join(media, f) for f in os.listdir(media))
kind = lambda exts: [f for f in files if os.path.splitext(f)[1].lower() in exts]
videos, images, audio = kind(VIDEO), kind(IMAGE), kind(AUDIO)
rows = [videos[::2], images, videos[1::2] or videos, audio, []]
cols = max(len(r) for r in rows)
clip = lambda p: {"path": p, "name": os.path.splitext(os.path.basename(p))[0]}
slots = [[clip(r[c]) if c < len(r) else None for c in range(cols)] for r in rows]

def preset(name, shader, secs=None, beats=None):
    time = {"Seconds": secs} if secs is not None else {"Beats": beats}
    return {"name": name, "shader": shader, "time": time, "easing": "InOut", "params": [], "favorite": False}

project = {
    "composition": {"width": 1920, "height": 1080, "bpm": 120.0, "layers": [{"name": n} for n in ["Video A", "Pictures", "Video B", "Sound", "Slides"]]},
    "decks": [{"name": "Soak", "slots": slots}],
    "transitions": [
        preset("Cut", "Cut", secs=0.0),
        preset("Crossfade", "Crossfade", secs=1.0),
        preset("Dip to Black", "Dip to Black", secs=1.0),
        preset("Wipe", "Wipe", beats=0.5),
        preset("Zoom", "Zoom", secs=1.0),
        preset("Pixelate", "Pixelate", beats=1.0),
    ],
    "materi": [{"title": os.path.basename(os.path.dirname(d)), "deck": d, "done": False} for d in decks],
    "presentation_layer": 4,
}
json.dump(project, open(out, 'w', encoding='utf-8'), indent=1)
print(f"{out}: {len(videos)} videos, {len(images)} pictures, {len(audio)} sound, {len(decks)} decks")
