#!/usr/bin/env python3
"""The module dependency graph of apps/inkpdf, computed from the sources the way the layering test
(`apps/inkpdf/src/architecture.rs`) reads them: the `crate::name` paths in production lines (`#[cfg(test)]`
items skipped), with the layers taken from its `LAYERS` table.

    python3 tools/deps/graph.py            write docs/deps/{skeleton,full,platform}.{dot,svg}   (needs Graphviz `dot`)
    python3 tools/deps/graph.py --check    exit 1 if the committed .dot files are out of date (no Graphviz needed)

Three views: `skeleton` (the transitive reduction: the edges that are not implied by others), `full` (every edge,
line weight = lines that use it) and `platform` (which OS backend is built where, from the files of `src/platform/`;
the `cfg` labels are written in this script and must be kept in step with `platform/mod.rs`).
"""
import math, os, re, subprocess, sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
SRC = os.path.join(ROOT, 'apps', 'inkpdf', 'src')
OUT = os.path.join(ROOT, 'docs', 'deps')
ARCH = open(f'{SRC}/architecture.rs').read()

layers = {}
order = ['Base', 'Model', 'Domain', 'Ui', 'Tools', 'Session', 'Adapter']
for m, l in re.findall(r'\("([a-z_]+)",\s*Layer::(\w+)\)', ARCH):
    layers[m] = l

def production_lines(text):
    lines = text.split('\n'); out = []; i = 0
    while i < len(lines):
        if lines[i].lstrip().startswith('#[cfg(test)]'):
            i += 1; depth = 0; opened = False
            while i < len(lines):
                item = lines[i]
                if not opened and item.lstrip().startswith('#['):
                    i += 1; continue
                depth += item.count('{') - item.count('}')
                opened = opened or '{' in item
                i += 1
                if (opened and depth <= 0) or (not opened and item.rstrip().endswith(';')):
                    break
            continue
        out.append((i, lines[i])); i += 1
    return out

def test_only(d):
    """Names of the modules a directory's parent file declares with #[cfg(test)] (`#[cfg(test)] mod name;`)."""
    names = set()
    for parent in (os.path.join(d, 'mod.rs'), d + '.rs'):
        if os.path.exists(parent):
            names |= set(re.findall(r'#\[cfg\(test\)\]\s*(?:pub(?:\([a-z]+\))?\s+)?mod\s+(\w+)\s*;', open(parent).read()))
    return names


def sources(d):
    skipped = test_only(d)
    for e in sorted(os.listdir(d)):
        p = os.path.join(d, e)
        if os.path.isdir(p):
            if e != 'tests' and e not in skipped: yield from sources(p)
        elif p.endswith('.rs') and e[:-3] not in skipped: yield p


skip = {'main','lib','bin','stress','architecture','testutil'}
mods = set()
for e in os.listdir(SRC):
    name = e[:-3] if e.endswith('.rs') else e
    if (os.path.isdir(f'{SRC}/{e}') or e.endswith('.rs')) and name not in skip: mods.add(name)

mods = sorted(mods)
edges = {}   # (a, b) -> number of lines in a that use b
for f in sources(SRC):
    rel = os.path.relpath(f, SRC)
    top = rel.split(os.sep)[0]; top = top[:-3] if top.endswith('.rs') else top
    if top not in mods: continue
    for _, line in production_lines(open(f).read()):
        if line.lstrip().startswith('//'): continue
        for m in re.finditer(r'crate::([A-Za-z0-9_]+)', line):
            n = m.group(1)
            if n != top and n in mods: edges[(top,n)] = edges.get((top,n),0)+1

fill={'Base':'#e8f1fb','Model':'#e3f4ea','Domain':'#fdf3d6','Ui':'#f6e4f3','Tools':'#fde5d4','Session':'#e4e1f8','Adapter':'#e6e6e6'}
edge_col={'Base':'#4a7fb5','Model':'#3a8f5c','Domain':'#b8860b','Ui':'#a14a96','Tools':'#d2691e','Session':'#6a5acd','Adapter':'#555555'}
desc={'Base':'base — plain data and arithmetic','Model':'model — what a drawing is made of','Domain':'domain — what devices and tools mean','Ui':'ui — buttons as data','Tools':'tools — what a press on the page does','Session':'session — editor: routing, chrome, history','Adapter':'adapter — window, GPU, files, OS'}

def reduce(es):
    out={a:set() for a in mods}
    for (a,b) in es: out[a].add(b)
    keep={}
    for (a,b),c in es.items():
        # redundant if b reachable from a via some other direct neighbour
        others=[x for x in out[a] if x!=b]
        seen=set(); st=list(others)
        while st:
            n=st.pop()
            if n in seen: continue
            seen.add(n); st+=list(out[n])
        if b not in seen: keep[(a,b)]=c
    return keep

TIP = ', tooltip="%d uses"'

def dot(es, title, reduced):
    L=['digraph G {','rankdir=BT; newrank=true; compound=true; splines=true; nodesep=0.35; ranksep=0.9; pad=0.4;',
       f'label=<<b>{title}</b>>; labelloc=t; fontsize=22; fontname="Helvetica";',
       'node [shape=box, style="rounded,filled", fontname="Helvetica", fontsize=13, margin="0.14,0.07", color="#888888"];',
       'edge [arrowsize=0.6];']
    for l in order:
        ms=[m for m in mods if layers[m]==l]
        L.append(f'subgraph cluster_{l} {{ label="{desc[l]}"; labeljust=l; fontname="Helvetica"; fontsize=14; style="rounded,filled"; fillcolor="{fill[l]}55"; color="{edge_col[l]}";')
        for m in ms:
            extra=' penwidth=2.5 color="#c0392b" fontcolor="#c0392b"' if m=='platform' else ''
            L.append(f'  "{m}" [fillcolor="white"{extra}];')
        L.append('}')
    for (a,b),c in sorted(es.items()):
        col=edge_col[layers[a]]
        pw=0.8+0.5*math.log2(c+1) if not reduced else 1.6
        L.append(f'"{a}" -> "{b}" [color="{col}99", penwidth={pw:.2f}{TIP % c}];')
    L.append('}')
    return '\n'.join(L)

full=dot(edges,f'InkPDF module dependencies (all {len(edges)} edges; arrows point at what is used; line weight = uses)',False)
red=reduce(edges)
skel=dot(red,f'InkPDF essential skeleton (transitive reduction: {len(red)} of {len(edges)} edges)',True)

# ---- platform routing view, from the real platform sources ----
EXT=['winit','windows_sys']
files=[]
for root,_,fs in os.walk(f'{SRC}/platform'):
    for f in fs:
        if f.endswith('.rs'): files.append(os.path.relpath(os.path.join(root,f),f'{SRC}/platform'))
files = [f for f in sorted(files) if f not in ('contract.rs', 'fake.rs')]  # (test-only: no backend of the program)
# the cfg labels below are written by hand: fail if platform/mod.rs does not declare the modules they name
mod_rs=open(f'{SRC}/platform/mod.rs').read()
for name in ('android','stubs','unix_dirs','linux','windows'):
    assert re.search(r'^mod '+name+';',mod_rs,re.M), f'platform/mod.rs no longer declares `mod {name};`: update the platform view in tools/deps/graph.py'
info={}
for f in files:
    txt=open(f'{SRC}/platform/{f}').read()
    lines=[l for l in txt.split('\n') if not l.lstrip().startswith('//')]
    body='\n'.join(lines)
    info[f]={'ext':sorted({c for c in EXT if re.search(r'\b'+c+r'::',body)}),
             'crate':sorted({m for m in re.findall(r'crate::([a-z_]+)',body) if m in mods and m!='platform'})}
nid=lambda f: 'p_'+f.replace('/','_').replace('.rs','')
cfg={'windows/mod.rs':'cfg(windows)','linux.rs':'cfg(unix, not android)','android/mod.rs':'cfg(target_os = android)','stubs.rs':'cfg(not(windows))','unix_dirs.rs':'cfg(not(windows))'}
colr={'windows':'#2e86c1','linux':'#27ae60','android':'#8e44ad','stubs':'#7f8c8d','unix_dirs':'#7f8c8d','motion_event':'#d68910','saf':'#16a085'}
def grp(f): return f.split('/')[0].replace('.rs','')
D=['digraph P {','rankdir=LR; newrank=true; splines=true; nodesep=0.3; ranksep=1.1; pad=0.4;',
   'label=<<b>Platform routing (compile-time): who is built where, and what each side talks to</b>>; labelloc=t; fontsize=22; fontname="Helvetica";',
   'node [shape=box, style="rounded,filled", fillcolor=white, fontname="Helvetica", fontsize=13, margin="0.14,0.07"];','edge [arrowsize=0.7, fontname="Helvetica", fontsize=11];']
D.append('subgraph cluster_shared { label="shared code (all OSes)"; style="rounded,filled"; fillcolor="#f4f6f7"; color="#aaaaaa"; fontname="Helvetica";')
for n,t in [('app','app\\nframe loop · files · State'),('main','main.rs\\nevent loop · args'),('editor','editor\\n(session)'),('input','input\\nPointerEvent · Mode · Sample')]:
    D.append(f'  "{n}" [label="{t}"];')
D.append('}')
D.append('subgraph cluster_plat { label="platform/  (adapter layer, picks one backend at compile time)"; style="rounded,filled"; fillcolor="#fdf2f0"; color="#c0392b"; fontname="Helvetica";')
D.append('  "platform_mod" [label="platform/mod.rs\\nPointerSource trait\\ntrait Platform · PointerSource\\nnative() picks the backend", penwidth=2.5, color="#c0392b"];')
for f in files:
    if f=='mod.rs': continue
    g=grp(f)
    D.append(f'  "{nid(f)}" [label="platform/{f}", color="{colr[g]}", penwidth=2];')
D.append('}')
D.append('subgraph cluster_ext { label="outside crates"; style="rounded,filled"; fillcolor="#f4f6f7"; color="#aaaaaa"; fontname="Helvetica";')
for c in EXT: D.append(f'  "x_{c}" [label="{c}", shape=component, fillcolor="#fffbe6"];')
D.append('}')
D+=['"main" -> "app"','"app" -> "platform_mod" [label="install_pointer · pump · drain\\ndetect_mode · motion_prefs\\nSystemClipboard · error_box · open_url", penwidth=2, color="#c0392b"]',
    '"app" -> "editor" [label="pointer_event · mouse_* · press"]','"main" -> "platform_mod" [label="attach_parent_console\\nreport_fatal", color="#c0392b"]',
    '"platform_mod" -> "input" [label="PointerEvent"]','"editor" -> "input"']
for f in files:
    if f=='mod.rs': continue
    g=grp(f); n=nid(f)
    if f in cfg: D.append(f'"platform_mod" -> "{n}" [label="{cfg[f]}", color="{colr[g]}", penwidth=2, fontcolor="{colr[g]}"]')
    for c in info[f]['ext']: D.append(f'"{n}" -> "x_{c}" [color="{colr[g]}", style=dashed]')
D.append('"platform_mod" -> "x_winit" [style=dashed, color="#c0392b"]')
# windows internals
D+=[f'"{nid("windows/mod.rs")}" -> "{nid(x)}" [color="{colr["windows"]}"]' for x in ['windows/pointer.rs','windows/native.rs','windows/posture.rs','windows/folders.rs','windows/counter.rs']]
D+=[f'"{nid("android/mod.rs")}" -> "{nid("android/reader.rs")}" [color="{colr["android"]}"]']
D+=[f'"{nid(os_file)}" -> "{nid(shared)}" [label="re-exports", color="{colr[g]}"]' for os_file,g in (('linux.rs','linux'),('android/mod.rs','android')) for shared in ('stubs.rs','unix_dirs.rs')]
D+=[f'"{nid("android/reader.rs")}" -> "{nid("motion_event.rs")}" [label="decodes with", color="{colr["android"]}"]']
D.append('{rank=same; "p_windows_mod"; "p_linux"; "p_android_mod";}')
D.append('}')
plat='\n'.join(D)

def main():
    check = '--check' in sys.argv[1:]
    views = {'skeleton': skel, 'full': full, 'platform': plat}
    stale = []
    for name, text in views.items():
        path = os.path.join(OUT, name + '.dot')
        old = open(path).read() if os.path.exists(path) else None
        if old != text + '\n':
            stale.append(name)
        if not check:
            os.makedirs(OUT, exist_ok=True)
            open(path, 'w').write(text + '\n')
            subprocess.run(['dot', '-Tsvg', path, '-o', os.path.join(OUT, name + '.svg')], check=True)
    print(f'{len(mods)} modules, {len(edges)} edges, {len(red)} in the skeleton')
    if check and stale:
        print('out of date: ' + ', '.join(stale) + ' (run: python3 tools/deps/graph.py)')
        sys.exit(1)
    print('docs/deps is up to date' if check else 'wrote ' + OUT)

main()
