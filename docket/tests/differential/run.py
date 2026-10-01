#!/usr/bin/env python3
"""The same command lines through the Python docket and this client, compared.

Two copies of one database, taken with sqlite3's .backup: the server runs on one, the Python docket on
the other, offline, writing its dump into an empty repository. Each command line runs through both, in
the same order, and stdout, stderr and the exit code are compared after the normalisations below.

    run.py --live DB --python SRC PROJECT [PROJECT ...]
    run.py --live DB --python SRC PROJECT -c 'show B12' -c 'next 3 --json'

SRC is the directory holding the Python `docket` package. The ids each command names are read from the
copy, so the plan fits any project.
"""

import argparse
import json
import os
import re
import shlex
import shutil
import socket
import sqlite3
import subprocess
import sys
import time

HOST = 'harness'
OWNER_KEY = 'harness-owner-key'
AGENT_KEY = 'harness-agent-key'
STAMP = re.compile(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ')
DURATION = re.compile(r'\b\d+[smhd]( \d+[hm])?\b')
BAR = re.compile(r'[#.]{10}')
# bm25 from the SQLite the server bundles can differ from the system SQLite's in the last bits.
SCORE = re.compile(r'("score": )(-?[0-9.e+-]+)')
# This host's own rows in the Python's database, which the server has no equal of: the loop's clock,
# the build hosts' stats, the chores, the roots and the dump not yet committed.
LOCAL_ROWS = (
    'DELETE FROM chores',
    'DELETE FROM roots',
    'DELETE FROM pending_dump',
    "DELETE FROM meta WHERE k LIKE 'last_tick %' OR k LIKE 'host_stats %' OR k LIKE 'plan_job %' "
    "OR k LIKE 'plan_seen %' OR k LIKE 'brake_cleared %' OR k LIKE 'pending_%' "
    "OR k IN ('loop_alive', 'last_sync', 'last_fetch')",
)
DUMP_LINE = re.compile(r'^.* (has no dump file; run docket sync|rows changed but not yet dumped.*)$', re.M)


def sh(args, **kw):
    return subprocess.run(args, capture_output=True, text=True, **kw)


class Pair:
    """Both commands, each on its own copy, run the same way."""

    def __init__(self, work, python_src, bin_dir):
        self.work = work
        self.python_src = python_src
        self.bin_dir = bin_dir
        self.server = None
        self.port = None

    def prepare(self, live, first_slug):
        base = os.path.join(self.work, 'base.db')
        for f in ('base.db', 'a.db', 'b.db'):
            for tail in ('', '-wal', '-shm'):
                p = os.path.join(self.work, f + tail)
                if os.path.exists(p):
                    os.remove(p)
        r = sh(['sqlite3', live, f'.backup {base}'])
        if r.returncode:
            sys.exit(f'backup failed: {r.stderr}')
        repo = os.path.join(self.work, 'pyrepo')
        shutil.rmtree(repo, ignore_errors=True)
        os.makedirs(repo)
        sh(['git', 'init', '-q'], cwd=repo)
        sh(['git', 'config', 'user.name', 'harness'], cwd=repo)
        sh(['git', 'config', 'user.email', 'harness@example.com'], cwd=repo)
        # The first Python command on a database may migrate it; it runs on the base so both copies agree.
        self.python(['-p', first_slug, 'status', '--json'], db=base)
        db = sqlite3.connect(base)
        for stmt in LOCAL_ROWS:
            db.execute(stmt)
        db.commit()
        db.close()
        shutil.copy(base, os.path.join(self.work, 'a.db'))
        shutil.copy(base, os.path.join(self.work, 'b.db'))
        for d in ('pyconf', 'rsconf/docket'):
            shutil.rmtree(os.path.join(self.work, d.split('/')[0]), ignore_errors=True)
            os.makedirs(os.path.join(self.work, d))
        with open(os.path.join(self.work, 'keys'), 'w') as f:
            f.write(f'{HOST} owner {OWNER_KEY}\n{HOST} agent {AGENT_KEY}\n')

    def start(self):
        with socket.socket() as s:
            s.bind(('127.0.0.1', 0))
            self.port = s.getsockname()[1]
        env = dict(os.environ, DOCKET_DB=os.path.join(self.work, 'a.db'),
                   DOCKET_KEYS=os.path.join(self.work, 'keys'), DOCKET_LISTEN=f'127.0.0.1:{self.port}')
        log = open(os.path.join(self.work, 'server.log'), 'w')
        self.server = subprocess.Popen([os.path.join(self.bin_dir, 'docket-server')], env=env, stdout=log,
                                       stderr=log)
        for _ in range(100):
            try:
                socket.create_connection(('127.0.0.1', self.port), timeout=0.2).close()
                return
            except OSError:
                time.sleep(0.1)
        sys.exit('the server did not start; see server.log')

    def stop(self):
        if self.server:
            self.server.terminate()
            self.server.wait()

    def python(self, args, stdin=None, env=None, cwd=None, db=None):
        e = dict(os.environ, DOCKET_DB=db or os.path.join(self.work, 'b.db'),
                 DOCKET_REPO=os.path.join(self.work, 'pyrepo'), DOCKET_OFFLINE='1', DOCKET_HOST=HOST,
                 XDG_CONFIG_HOME=os.path.join(self.work, 'pyconf'), PYTHONPATH=self.python_src)
        e.pop('DOCKET_PROJECT', None)
        e.pop('DOCKET_JOB', None)
        e.update(env or {})
        return sh([sys.executable, '-m', 'docket', *args], input=stdin, env=e, cwd=cwd or self.work)

    def rust(self, args, stdin=None, env=None, cwd=None):
        e = dict(os.environ, DOCKET_SERVER=f'http://127.0.0.1:{self.port}', DOCKET_KEY=OWNER_KEY,
                 XDG_CONFIG_HOME=os.path.join(self.work, 'rsconf'))
        e.pop('DOCKET_PROJECT', None)
        e.pop('DOCKET_JOB', None)
        e.pop('DOCKET_REPO', None)
        e.update(env or {})
        return sh([os.path.join(self.bin_dir, 'docket'), *args], input=stdin, env=e, cwd=cwd or self.work)


def verb_of(args):
    """The verb of a command line, past the flags that take a value."""
    skip = False
    for a in args:
        if skip:
            skip = False
        elif a in ('-p', '--project', '--branch'):
            skip = True
        elif not a.startswith('-'):
            return a
    return ''


def normalise(args, out, err, code, python):
    """What may differ between two runs a second apart, what only the Python's own dump says, and the
    wording of an argument error, which argparse and clap each word their own way."""
    out, err = STAMP.sub('<T>', out), STAMP.sub('<T>', err)
    out = SCORE.sub(lambda m: m.group(1) + f'{float(m.group(2)):.12g}', out)
    if code == 2:
        err = '<argument error>\n'
    if verb_of(args) in ('status', ''):
        out = DURATION.sub('<D>', BAR.sub('<BAR>', out))
        if python:
            out = without_dump(out)
    if python and 'check' in args:
        out = DUMP_LINE.sub('', out).replace('\n\n', '\n')
        if '--json' in args:
            problems = [p for p in json.loads(out or '[]') if 'dump' not in p]
            out = json.dumps(problems, indent=2) + '\n'
            code = 1 if problems else 0
        elif not out.strip():
            out, code = 'clean\n', 0
        elif out.startswith('\n'):
            out = out[1:]
    return out, err, code


def without_dump(text):
    """The status text with the Python's dump lines taken out of its Check block, and the block with them
    when nothing else is in it."""
    lines = [x for x in text.split('\n') if not DUMP_LINE.match(x)]
    out = []
    for i, x in enumerate(lines):
        if x == 'Check:' and (i + 1 == len(lines) or not lines[i + 1].startswith('  ')):
            if out and out[-1] == '':
                out.pop()
            continue
        out.append(x)
    return '\n'.join(out)


class Ids:
    """The items a plan names, read from the copy before anything is written."""

    def __init__(self, db, slug):
        self.db, self.slug = db, slug
        keys = json.loads(db.execute('SELECT keys FROM projects WHERE slug=?', (slug,)).fetchone()[0])
        self.keys = {k['kind']: [x['key'] for x in keys if x['kind'] == k['kind']] for k in keys}
        work = self.keys.get('work', [])
        self.work = self.ready(work, 3)
        self.done = self.one("state='done'")
        self.dropped = self.one("state='dropped'")
        self.package = self.one("state='open'", 'package')
        self.concept = self.one("state='open'", 'concept')
        self.idea = self.one("state='open'", 'idea')
        self.audit = self.one("1=1", 'audit')
        self.question = self.one("state='open' AND decision IS NULL AND turn='user'", 'decision')
        self.waiting = self.one("state='open' AND wait_on='item'")
        pk = self.keys.get('package', [None])[0]
        top = self.db.execute('SELECT MAX(num) FROM items WHERE project=? AND key=?', (slug, pk)).fetchone()
        self.new_package = f'{pk}{(top[0] or 0) + 1}' if pk else None
        self.group = self.value('group_name', "group_name IS NOT NULL")
        self.theme = self.value('theme', "theme IS NOT NULL AND state='open'")
        path = self.db.execute("SELECT l.to_path FROM links l JOIN items i ON i.rid=l.rid WHERE i.project=? "
                               "AND l.kind='cites_file' ORDER BY l.rowid LIMIT 1", (slug,)).fetchone()
        self.prefix = path[0].split('/')[0] if path else None
        title = self.db.execute('SELECT title FROM items WHERE project=? ORDER BY rid LIMIT 1', (slug,)).fetchone()
        words = re.findall(r'[A-Za-z]{5,}', title[0]) if title else []
        self.word = words[0] if words else None

    def keyed(self, kind):
        ks = self.keys.get(kind, [])
        return f"key IN ({','.join(repr(k) for k in ks)})" if ks else '0'

    def one(self, where, kind=None):
        sql = f'SELECT id FROM items WHERE project=? AND {where}'
        if kind:
            sql += f' AND {self.keyed(kind)}'
        row = self.db.execute(sql + ' ORDER BY rid DESC LIMIT 1', (self.slug,)).fetchone()
        return row[0] if row else None

    def value(self, col, where):
        row = self.db.execute(f'SELECT {col} FROM items WHERE project=? AND {where} ORDER BY rid LIMIT 1',
                              (self.slug,)).fetchone()
        return row[0] if row else None

    def ready(self, work, n):
        if not work:
            return []
        rows = self.db.execute(
            f"SELECT id FROM items i WHERE project=? AND {self.keyed('work')} AND state='open' AND turn='agent' "
            "AND claim_branch IS NULL AND wait_on IS NULL AND scope IS NULL AND conflict=0 "
            "ORDER BY rid DESC LIMIT ?", (self.slug, n)).fetchall()
        return [r[0] for r in rows]


def reads(i):
    """Every read, with and without --json where it takes it, and the refusals a read can give."""
    out = ['', '--json', 'status', 'status --json', 'next', 'next 3 --json', 'next --all', 'next --priority high',
           'next --scope inbox', 'next --scope later --json', 'complex', 'complex 3 --json', 'todo', 'todo --json',
           'wip', 'wip --json', 'waiting', 'waiting --json', 'blocked --on condition', 'questions', 'q --json',
           'research', 'research --json', 'derived', 'derived 5 --json', 'done 5', 'done 3 --json',
           'dropped 3', 'dropped 2 --json', 'groups', 'groups --json', 'projects', 'projects --json',
           'graph', 'graph --dot', 'graph --no-files', 'check', 'check --json', 'audit --orphans',
           'audit --orphans --json', 'stale', 'show ZZ', 'show B999999', 'audit', 'next --under QQ',
           'log not-an-id']
    for k in i.keys.get('work', [])[:1]:
        out += [f'next --key {k.lower()}', f'done 3 --key {k}']
    for item in [*i.work, i.done, i.dropped, i.package, i.concept, i.idea, i.audit, i.question, i.waiting]:
        if item:
            out += [f'show {item}', f'show {item} --json', f'log {item}', f'deps {item}']
    for item in [i.work[0] if i.work else None, i.package, i.done]:
        if item:
            out += [f'log {item} --json', f'deps {item} --json', f'similar {item}', f'similar {item} 3 --json',
                    f'similar {item} --state done']
    for item in [i.package, i.concept, i.idea, i.audit]:
        if item:
            out += [f'audit {item}', f'audit {item} --json', f'next --under {item}']
    if i.group:
        out += [shlex.join(['groups', i.group]), shlex.join(['audit', '--group', i.group]),
                shlex.join(['audit', '--group', i.group, '--json'])]
    if i.theme:
        out += [shlex.join(['audit', '--theme', i.theme]), shlex.join(['next', '--theme', i.theme]),
                shlex.join(['questions', '--theme', i.theme])]
    if i.word:
        out += [f'search {i.word}', f'search {i.word} --json', f'search {i.word} --state open -n 3',
                f'search "{i.word} OR fix" --raw -n 5']
    if i.prefix:
        out += [shlex.join(['files', i.prefix]), shlex.join(['files', i.prefix, '--json']),
                shlex.join(['files', i.prefix, '--state', 'open'])]
    return out


def writes(i):
    """A sequence of writes and the reads between them, refusals included. Each step changes the copy."""
    if len(i.work) < 2:
        return []
    w, w2 = i.work[:2]
    out = [
        ("new T 'harness ticket' --body -", '- **Evidence.** `src/harness.rs:3`\n'),
        ("new B 'harness bug' --priority high --complexity low --theme harness", None),
        ("new B 'harness bug' --json", None),
        ("add 'inbox ticket from the harness'", None),
        ("add 'inbox with key' --key T --from job-1 --json", None),
        ("new ZZ 'unknown key'", None),
        (f'defer {w2} --why later', None), (f'defer {w2}', None), (f'pull {w2} --json', None),
        (f'start {w} --branch audit/h-1', None), (f'start {w} --branch audit/h-1', None),
        (f'start {w} --branch audit/h-2', None), ('wip', None), ('wip --json', None),
        (f'release {w} --branch audit/h-2', None), (f'release {w} "back" --branch audit/h-1 --json', None),
        (f'release {w}', None), (f'start {w} --branch audit/h-1 --runner codex --job j1 --model m', None),
        (f'start {w2} --job j2', None), (f'show {w}', None),
        (f'close {w} --branch audit/h-1', None), (f'close {w} abc1234 --branch audit/h-1 --gates nope', None),
        (f'close {w} abc1234 --branch audit/h-1 --gates "passed: all"', None), (f'close {w} x', None),
        (f'reopen {w}', None), (f'reopen {w} again --json', None), (f'reopen {w} again', None),
        (f'wait {w}', None), (f'wait {w} --until rain --on {w2}', None), (f'wait {w} --until rain', None),
        (f'wait {w} --until snow', None), (f'resume {w} dry', None), (f'resume {w}', None),
        (f'wait {w} --on {w2} --json', None), ('waiting', None), (f'show {w2}', None), (f'deps {w2}', None),
        (f'resume {w}', None), (f'ask {w} "which one"', None), (f'start {w}', None), ('todo', None),
        (f'reply {w} "this one"', None), (f'reply {w} again', None), (f'rate {w} high', None),
        (f'rate {w} low --json', None), (f'priority {w} {w2} critical', None), (f'priority {w} normal --json', None),
        (f'edit {w}', None), (f'edit {w} --set theme=harness --append "a note"', None),
        (f"edit {w} --set 'title=A harness title' --set rank=3 --set group=hg", None),
        (f'edit {w} --set state=done', None), (f'edit {w} --body -', 'Replaced body `src/x.py:2`\n'),
        (f'edit {w} --set tags=a,b --json', None), ('groups hg', None),
        (f'link {w} related {w2}', None), (f'link {w} {w2} related {w2}', None),
        (f'link {w} related {w2} --remove', None), (f'link {w} opened {w2}', None), (f'deps {w}', None),
        (f'link {w} opened {w2} --remove', None), (f'link {w} sideways {w2}', None),
        (f'decide {w} "use the queue" --basis "CID1, the queue owns order"', None), ('derived 3', None),
        (f'block {w} --until "the alias"', None), (f'resume {w}', None), (f'park {w} "by alias"', None),
        (f'reply {w} back', None), (f'manual {w} "by the other alias" --json', None), (f'reply {w} back', None),
        (f'start {w} --branch audit/h-3', None), (f'built {w} def5678 --branch audit/h-3', None),
        (f'reopen {w} "after built"', None),
        (f'answer {w} "yes"', None), (f'drop {w2}', None), (f'drop {w2} "not needed"', None),
        (f'drop {w2} again', None), (f'show {w2} --json', None), ('key zz work "harness key"', None),
        ('key ZZ decision "changed"', None), ('key toolong work "x"', None), ('reindex', None),
        (f'fold {w} {w2}', None),
    ]
    if i.question:
        q = i.question
        out += [(f'answer {q} "the harness choice"', None), (f'close {q}', None),
                (f'answer {q} "again" --derived "a prior"', None), (f'close {q} "opened {w}" --json', None),
                (f'show {q}', None)]
    if i.package and i.new_package:
        pk, new = i.package, i.new_package
        out += [(f"new {new.rstrip('0123456789')} 'harness package'", None), (f'link {w} opened {new}', None),
                (f'link {w} opened {pk}', None), (f'show {new}', None), (f'start {new}', None),
                (f'fold {new} {new}', None), (f'fold {pk} {new}', None), (f'show {pk}', None),
                (f'audit {pk}', None), (f'start {pk}', None)]
    return out


def compare(pair, line, stdin=None, env=None, cwd=None):
    args = shlex.split(line)
    py = pair.python(args, stdin, env, cwd)
    rs = pair.rust(args, stdin, env, cwd)
    a = normalise(args, py.stdout, py.stderr, py.returncode, True)
    b = normalise(args, rs.stdout, rs.stderr, rs.returncode, False)
    return a == b, a, b


def show_diff(line, a, b):
    import difflib
    print(f'--- differs: docket {line}')
    for name, x, y in (('stdout', a[0], b[0]), ('stderr', a[1], b[1])):
        if x != y:
            diff = difflib.unified_diff(x.splitlines(), y.splitlines(), 'python', 'rust', lineterm='', n=1)
            print(f'  {name}:')
            for d in list(diff)[2:40]:
                print('    ' + d)
    if a[2] != b[2]:
        print(f'  exit: python {a[2]}, rust {b[2]}')


def checkout_steps(pair, slug, item):
    """The steps that read the checkout: a project resolved from a repository, a root bound by hand, a
    sha read from the claimed branch, a close refused mid-merge, and a write refused inside a job."""
    repo = os.path.join(pair.work, 'checkout')
    shutil.rmtree(repo, ignore_errors=True)
    os.makedirs(repo)
    for cmd in (['init', '-q', '-b', 'main'], ['config', 'user.name', 'h'], ['config', 'user.email', 'h@x'],
                ['remote', 'add', 'origin', f'git@example.com:{slug}.git'], ['commit', '-q', '--allow-empty', '-m', 'a'],
                ['branch', 'audit/h-9']):
        sh(['git', *cmd], cwd=repo)
    steps = [('status --json', None, None, repo), ('bind', None, None, repo), ('next 2', None, None, repo),
             (f'bind {slug} --root {repo}', None, None, None), ('stale --open-only', None, None, repo),
             ('projects', None, None, None)]
    if item:
        steps += [(f'start {item} --branch audit/h-9', None, None, repo), (f'close {item}', None, None, repo),
                  (f'reopen {item} "for the merge check"', None, None, repo)]
    other = os.path.join(pair.work, 'elsewhere')
    shutil.rmtree(other, ignore_errors=True)
    os.makedirs(other)
    sh(['git', 'init', '-q'], cwd=other)
    sh(['git', 'remote', 'add', 'origin', 'git@example.com:harness-new/created.git'], cwd=other)
    steps += [('status --json', None, None, other), ('projects --json', None, None, None),
              ('-p no/such next', None, None, None),
              ("new T 'from a job'", None, {'DOCKET_JOB': 'job-7'}, None),
              ('next 1', None, None, '/')]
    return steps


def merge_steps(pair, slug, item):
    """A close inside a repository part way through a merge is refused before anything is read."""
    if not item:
        return []
    repo = os.path.join(pair.work, 'checkout')
    with open(os.path.join(repo, '.git', 'MERGE_HEAD'), 'w') as f:
        f.write('0' * 40 + '\n')
    return [(f'-p {slug} close {item} abc1234', None, None, repo),
            (f'-p {slug} close {item} abc1234 --force --json', None, None, repo)]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--live', required=True, help='the database to take the copies from, with .backup')
    ap.add_argument('--python', required=True, help='the directory holding the Python docket package')
    ap.add_argument('--bin', default='target/debug', help='the directory holding docket and docket-server')
    ap.add_argument('--work', default=os.path.expanduser('~/.cache/docket-differential'))
    ap.add_argument('-c', '--command', action='append', help='one command line, run instead of the plan')
    ap.add_argument('--reuse', action='store_true', help='keep the copies from the last run')
    ap.add_argument('projects', nargs='+')
    a = ap.parse_args()
    os.makedirs(a.work, exist_ok=True)
    pair = Pair(a.work, os.path.abspath(a.python), os.path.abspath(a.bin))
    if not a.reuse:
        pair.prepare(a.live, a.projects[0])
    pair.start()
    total = same = 0
    exits = {}
    try:
        for slug in a.projects:
            db = sqlite3.connect(os.path.join(a.work, 'b.db'))
            ids = Ids(db, slug)
            db.close()
            if a.command:
                steps = [(f'-p {slug} {c}', None, None, None) for c in a.command]
            else:
                steps = [(f'-p {slug} {c}', None, None, None) for c in reads(ids)]
                steps += [(f'-p {slug} {c}', s, None, None) for c, s in writes(ids)]
                steps += checkout_steps(pair, slug, ids.work[2] if len(ids.work) > 2 else None)
                steps += merge_steps(pair, slug, ids.work[0] if ids.work else None)
            print(f'== {slug}: {len(steps)} command lines')
            for line, stdin, env, cwd in steps:
                ok, x, y = compare(pair, line, stdin, env, cwd)
                total += 1
                same += ok
                exits[x[2]] = exits.get(x[2], 0) + 1
                if not ok:
                    show_diff(line, x, y)
    finally:
        pair.stop()
    print(f'\n{same} of {total} command lines gave the same stdout, stderr and exit code')
    print('by the Python\'s exit code: ' + ', '.join(f'{n} exited {c}' for c, n in sorted(exits.items())))
    return 0 if same == total else 1


if __name__ == '__main__':
    sys.exit(main())
