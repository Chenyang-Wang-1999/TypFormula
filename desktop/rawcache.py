"""Stable Raw identities and script edit sessions, independent of byte offsets."""
from difflib import SequenceMatcher

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
        def identity(n):return (n.get('text'),n.get('warmup_key'),n.get('origin'))
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
                if node.get('kind')!='script':continue
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
