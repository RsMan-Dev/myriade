"""Reads the level events and sounds a joueur-automate bot captured (`Bot.capture`, rows `id`, `hex` = the body).

Protocol 777 ids: 47 ClientboundLevelEventPacket, 119 ClientboundSoundPacket. The sound's pitch and seed come from
the level random, whose order a whole-server comparison does not pin: they are left out of the rows.
"""
import struct

LEVEL_EVENT = 47
SOUND = 119


def varint(data, at):
    value = shift = 0
    while True:
        byte = data[at]
        at += 1
        value |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return value, at
        shift += 7


def level_event(hex_body):
    """(kind, [x, y, z], data, global) of a level event body: a big-endian int, a packed block position, an int, a flag."""
    body = bytes.fromhex(hex_body)
    kind, = struct.unpack_from('>i', body, 0)
    packed, = struct.unpack_from('>q', body, 4)
    x = packed >> 38
    z = (packed >> 12) & 0x3FFFFFF
    if z >= 1 << 25:
        z -= 1 << 26
    y = packed & 0xFFF
    if y >= 1 << 11:
        y -= 1 << 12
    data, = struct.unpack_from('>i', body, 12)
    return kind, [x, y, z], data, bool(body[16])


def sound(hex_body):
    """(event id, category, [x, y, z] as sent, volume bits) of a sound body: a holder (its id plus one, 0 when inline),
    the category, positions times eight, the volume float, the pitch float and the seed."""
    body = bytes.fromhex(hex_body)
    holder, at = varint(body, 0)
    if holder == 0:
        raise ValueError('inline sound event: not read here')
    category, at = varint(body, at)
    x, y, z = struct.unpack_from('>iii', body, at)
    volume, = struct.unpack_from('>I', body, at + 12)
    return holder - 1, category, [x, y, z], f'{volume:08x}'


def captured(packets):
    """The level events and the sounds of a capture, in the order they arrived, as text rows."""
    levels = [str(level_event(p['hex'])) for p in packets if p['id'] == LEVEL_EVENT]
    sounds = [str(sound(p['hex'])) for p in packets if p['id'] == SOUND]
    return levels, sounds
