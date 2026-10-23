"""Drafts the disc song details tables (disc_meta/<edition>.tsv) for the
IIDX 16 discs from the iidx16-bms Textage mapping, for a person to check.

The player reads the tables (crates/cs-disc/src/meta.rs has the format) to
show real titles, subtitles, artists and genres, and to find the loading
card where the disc gives no rule for it (EMPRESS). The tables hold data
taken from Textage, so they stay on the user's machine: disc_meta/ is not
part of the repository.

Inputs (from the iidx16-bms work):
  --map       song_map_final.csv: disc song id -> Textage key, title, artist, genre
  --titletbl  titletbl.js (the UTF-8 copy): subtitles, by Textage key
  --cards     empress_map.py: EMPRESS card container -> title, read off the cards by eye

Rows that were not matched by title and levels together are marked
"CHECK" in the note column; review those first. Re-running overwrites the
tables, so keep hand edits in the tables themselves afterwards, or in a copy.

Usage: python scripts/disc_meta_from_textage.py --map <csv> --titletbl <js> [--cards <py>] [--out disc_meta]
"""
import argparse
import csv
import os
import re
import runpy

EDITIONS = {'EMPRESS': 'iidx16-empress', 'PREMIUM BEST': 'iidx16-premium-best'}
COLUMNS = ['id', 'disc_title', 'title', 'subtitle', 'artist', 'genre', 'card', 'note']

# Decisions made by hand in the iidx16-bms work (docs/stagefile_extraction.md):
# card 357's title is in kanji (TECHNO BANCHO), and NO CRIME J uses NO CRIME's card.
CARD_BY_TITLE_EXTRA = {'TECHNO BANCHO': 357, 'NO CRIME J': 346}


def key(title):
    """Titles compared without case, spaces or punctuation."""
    return re.sub(r'[^0-9A-Z]', '', title.upper())


def read_titletbl(path):
    """Textage key -> [genre, artist, title, subtitle] (subtitle may be missing)."""
    text = open(path, encoding='utf-8').read()
    text = re.sub(r'/\*.*?\*/', '', text, flags=re.S)
    rows = {}
    for m in re.finditer(r"^'([^']+)'\s*:\s*\[(.*)\]\s*,?\s*$", text, flags=re.M):
        strings = [s.replace('\\"', '"').replace('\\\\', '\\')
                   for s in re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(2))]
        rows[m.group(1)] = strings
    return rows


def clean(value):
    """One line of plain text: Textage marks some subtitles up in HTML."""
    value = re.sub(r'<[^>]+>', '', (value or '').replace('\\/', '/'))
    return re.sub(r'[\t\r\n]+', ' ', value).strip()


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--map', required=True)
    ap.add_argument('--titletbl', required=True)
    ap.add_argument('--cards')
    ap.add_argument('--out', default='disc_meta')
    args = ap.parse_args()

    titles = read_titletbl(args.titletbl)
    cards = {}
    if args.cards:
        card_title = runpy.run_path(args.cards)['CARD_TITLE']
        cards = {key(t): c for c, t in card_title.items()}
        cards.update({key(t): c for t, c in CARD_BY_TITLE_EXTRA.items()})

    tables = {edition: [] for edition in EDITIONS}
    with open(args.map, encoding='utf-8-sig', newline='') as f:
        rows = list(csv.reader(f))
    for r in rows[1:]:
        if len(r) < 8 or r[0] not in EDITIONS:
            continue
        disc, song_id, disc_title, _levels, tkey, title, artist, genre = r[:8]
        match = ','.join(r[8:]).strip()
        subtitle = ''
        row = titles.get(tkey)
        if row and len(row) >= 4:
            subtitle = row[3]
        card = ''
        if disc == 'EMPRESS':
            card = str(cards.get(key(disc_title), ''))
        notes = ['textage ' + tkey] if tkey else []
        if not match.startswith('title+levels'):
            notes.append('CHECK ' + match)
        if disc == 'EMPRESS' and args.cards and not card:
            notes.append('CHECK no card')
        tables[disc].append([song_id, disc_title, title, subtitle, artist, genre, card, '; '.join(notes)])

    os.makedirs(args.out, exist_ok=True)
    for disc, slug in EDITIONS.items():
        path = os.path.join(args.out, slug + '.tsv')
        with open(path, 'w', encoding='utf-8', newline='\n') as f:
            f.write('# %s song details, drafted by scripts/disc_meta_from_textage.py.\n' % disc)
            f.write('# Check the rows marked CHECK. An empty cell keeps what the disc says.\n')
            f.write('\t'.join(COLUMNS) + '\n')
            for row in sorted(tables[disc], key=lambda r: int(r[0])):
                f.write('\t'.join(clean(v) for v in row) + '\n')
        checks = sum(1 for row in tables[disc] if 'CHECK' in row[-1])
        print('%s: %d songs, %d to check -> %s' % (disc, len(tables[disc]), checks, path))


if __name__ == '__main__':
    main()
