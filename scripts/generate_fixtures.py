"""Generate original, dependency-free M0 fixtures and application icons."""
from pathlib import Path
import struct
import zlib

ROOT = Path(__file__).resolve().parent.parent


def png(width, height, pixel):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    rows = b''.join(b'\x00' + bytes(c for x in range(width) for c in pixel(x, y)) for y in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def icon(x, y):
    if (70 <= x <= 94 and 60 <= y <= 188) or (70 <= x <= 185 and 164 <= y <= 188):
        return (19, 41, 30, 255)
    if (132 <= x <= 150 and 69 <= y <= 144) or (132 <= x <= 188 and 69 <= y <= 87) or (132 <= x <= 177 and 100 <= y <= 118):
        return (19, 41, 30, 255)
    return (162, 232, 189, 255)


def scene(x, y):
    # Synthetic calibration scene: horizon, road corridor, lane marks and 120px grid.
    if x % 120 == 0 or y % 120 == 0:
        return (112, 156, 153, 255)
    if y < 430:
        return (48 + y // 18, 86 + y // 14, 107 + y // 12, 255)
    road_half = (y - 300) * 1.1
    if abs(x - 960) < road_half:
        if abs(x - 960) < 9 and y % 150 < 85:
            return (239, 212, 137, 255)
        if abs(abs(x - 960) - road_half + 30) < 5:
            return (217, 231, 223, 255)
        return (60, 68, 77, 255)
    return (53 + y // 65, 91 + y // 75, 72 + y // 90, 255)


if __name__ == '__main__':
    icons = ROOT / 'src-tauri/icons'
    icons.mkdir(parents=True, exist_ok=True)
    content = png(256, 256, icon)
    (icons / 'icon.png').write_bytes(content)
    (icons / 'icon.ico').write_bytes(struct.pack('<HHH', 0, 1, 1) + struct.pack('<BBBBHHII', 0, 0, 0, 0, 1, 32, len(content), 22) + content)
    fixtures = ROOT / 'tests/fixtures'
    fixtures.mkdir(parents=True, exist_ok=True)
    (fixtures / 'calibration-1920x1080.png').write_bytes(png(1920, 1080, scene))
    (fixtures / 'corrupt.png').write_bytes(b'not an image\x00')
