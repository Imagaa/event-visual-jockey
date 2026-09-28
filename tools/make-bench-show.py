"""G2 bench show: 4 HAP 1080p layers, 2 effects per layer, 2 outputs.
`python tools/make-bench-show.py <out.vjproj> [media-dir]` then `evj --bench 60 <out.vjproj>` (column 1 plays).
"""
import json
import os
import sys

out = sys.argv[1]
media = os.path.abspath(sys.argv[2] if len(sys.argv) > 2 else 'media')
clips = ['bench_hap1.mov', 'bench_hapq.mov', 'bench_hap5.mov', 'bench_hap1b.mov']
fx = lambda name, **p: {"name": name, "params": [{"name": k, "value": v} for k, v in p.items()]}
layers = [{"name": f"HAP {i + 1}", "opacity": 1.0 if i == 0 else 0.6, "blend": "Alpha" if i < 2 else "Screen",
           "effects": [fx("Hue Shift", hue=30.0 * i), fx("RGB Shift", amount=6.0)]} for i in range(4)]
project = {
    "composition": {"width": 1920, "height": 1080, "bpm": 120.0, "layers": layers},
    "decks": [{"name": "Bench", "slots": [[{"path": os.path.join(media, c), "name": c}] for c in clips]}],
    "outputs": [{"name": "Main"}, {"name": "Side", "source": {"Layer": 3}}],
}
json.dump(project, open(out, 'w', encoding='utf-8'), indent=1)
print(out)
