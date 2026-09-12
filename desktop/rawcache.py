"""Stable Raw identities, script edit sessions, and the fragment cache policy."""
import hashlib,os,re
from difflib import SequenceMatcher

# Which fragment images may be kept for a later request. `all` is the editor's
# behaviour: every fragment is reused by its source text. `plain` is an
# experiment switch, not a setting -- it exists to measure what dropping the
# reuse of the fragments that hold a call costs.
MODE = os.environ.get("TYPFORMULA_RAW_CACHE", "all").strip()

# One identifier followed by an opening parenthesis. The text of a Raw is the
# source the renderer is asked for, so `cancel(a)`, `mat(1, 2)` and `#pd(f, x)`
# read as calls while `partial`, `#f` and `x_1` do not. This is a guess about
# source text, not a parse: a call that is written `f (x)` costs one extra render.
CALL = re.compile(r"[^\W\d]\w*\s*\(")

def contains_call(text):
    return CALL.search(text) is not None

def reusable(text):
    """Whether an image asked for with this source may answer a later request."""
    if MODE != "plain":return True
    return not contains_call(text)

def raw_key(node):
    """The identity of one fragment's image, as the cache stores it.

    A fragment is compiled where it stands, and almost always its own source is the
    whole story: `cal(A)` is the same box in a numerator and in a denominator. The
    exception this editor knows is an attachment's base -- `stretch(->)^x`
    stretches to the width of `x`, so the same base source is a different picture
    under a different script. A fragment that sits in a `script` therefore carries
    that script's shape as `_context` (a digest the window stamps), and it is part
    of the key. Everything else shares one image per source text, wherever it stands.
    """
    text=node.get('text','')
    request=node.get('render_request',{})
    if request.get('call'):
        return ('raw-instance',text,node.get('origin'),node.get('_call_identity',tuple(request['call'])),request.get('occurrence',0),node.get('_context'))
    return ('raw',text,node['_context']) if node.get('_context') else ('raw',text)

def signature_digest(value):
    """A short key for one view subtree, for the fragments whose box follows it."""
    return hashlib.blake2b(repr(value).encode('utf-8'),digest_size=8).hexdigest()

def nodes(view):
    yield view
    for child in view.get('children',[]):yield from nodes(child)

def signature(node):
    return (node.get('kind'),node.get('text'),node.get('columns'),tuple(signature(c) for c in node.get('children',[]) if c['kind'] not in ('stop','draft-caret')))

class RawCache:
    def __init__(self):self.serial=0;self.edits={}

    def bind(self,old,new):
        before=[n for n in nodes(old) if n.get('kind')=='raw']
        after=[n for n in nodes(new) if n.get('kind')=='raw']
        def identity(n):return (n.get('text'),n.get('origin'))
        matcher=SequenceMatcher(a=list(map(identity,before)),b=list(map(identity,after)),autojunk=False)
        for block in matcher.get_matching_blocks():
            for i in range(block.size):
                key=before[block.a+i].get('_raw_key')
                if key:after[block.b+i]['_raw_key']=key
        for node in after:
            if '_raw_key' not in node:
                self.serial+=1;node['_raw_key']=f'native-raw:{self.serial}'

    def rebind(self,old,new):
        before=old.get('formulas',[])
        for i,formula in enumerate(new.get('formulas',[])):
            self.bind(before[i].get('view',{}) if i<len(before) else {},formula.get('view',{}))

    def track(self,state,cache,leaving=False):
        current={}
        if state:
            for node in nodes(state['view']):
                if node.get('kind')!='scripts':continue
                raw=tuple((n.get('_raw_key'),n.get('text','')) for n in nodes(node['children'][0]) if n.get('kind')=='raw')
                if not raw:continue
                slots=node['children'][1:]
                if any(n.get('active') for slot in slots for n in nodes(slot)):
                    current[raw]=signature({'kind':'scripts','children':slots})
        invalidated=set()
        for keys,(initial,latest) in list(self.edits.items()):
            latest=current.get(keys,latest)
            if leaving or keys not in current:
                if latest!=initial:
                    for key,text in keys:
                        cache.pop(key,None);invalidated.add((key,text))
                del self.edits[keys]
            else:self.edits[keys]=(initial,latest)
        if not leaving:
            for keys,value in current.items():self.edits.setdefault(keys,(value,value))
        return invalidated
