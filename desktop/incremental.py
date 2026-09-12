"""Merge projections around Typst's own incrementally reparsed syntax range."""
from copy import deepcopy

def _range(a,b,start,end,delta):
    if b<=start:return a,b
    if a>=end:return a+delta,b+delta
    if a<=start and end<=b:return a,b+delta
    return None

def _shift_id(value,start,end,delta):
    if not isinstance(value,str):return value
    parts=value.split(':')
    if len(parts)<2 or not parts[0].isdigit() or not parts[1].isdigit():return value
    shifted=_range(int(parts[0]),int(parts[1]),start,end,delta)
    if not shifted:return value
    parts[0],parts[1]=map(str,shifted);return ':'.join(parts)

# Display identities move with the edit, so only the nodes that actually move are
# rebuilt. Untouched subtrees are shared with the previous projection instead of
# being deep copied once per keystroke.
def _shift_view(node,start,end,delta):
    result=node
    if isinstance(node.get('source_range'),list):
        shifted=_range(*node['source_range'],start,end,delta)
        if shifted and list(shifted)!=node['source_range']:
            result={**result,'source_range':list(shifted)}
    if 'render_id' in node:
        moved=_shift_id(node['render_id'],start,end,delta)
        if moved!=node['render_id']:result={**result,'render_id':moved}
    children=node.get('children')
    if children:
        shifted=[_shift_view(child,start,end,delta) for child in children]
        if any(new is not old for new,old in zip(shifted,children)):result={**result,'children':shifted}
    return result

def _shift_formula(formula,start,end,delta,new_source):
    """Shift one formula, or return None when the edit replaced it."""
    bounds=_range(formula['start'],formula['end'],start,end,delta)
    if not bounds:return None
    result=formula
    if bounds!=(formula['start'],formula['end']):result={**result,'start':bounds[0],'end':bounds[1]}
    render=formula.get('render')
    if render:
        moved={**render,'source':new_source}
        for group in ('raw','formulas'):
            items=render.get(group)
            if not items:continue
            shifted=[]
            for item in items:
                item_bounds=_range(item['start'],item['end'],start,end,delta)
                item_id=_shift_id(item.get('id'),start,end,delta)
                if item_bounds and (item_bounds!=(item['start'],item['end']) or item_id!=item.get('id')):
                    shifted.append({**item,'start':item_bounds[0],'end':item_bounds[1],'id':item_id})
                else:shifted.append(item)
            moved[group]=shifted
        result={**result,'render':moved}
    if 'view' in formula:
        view=_shift_view(formula['view'],start,end,delta)
        if view is not formula['view']:result={**result,'view':view}
    return result

def merge(previous,syntax,new_source,start,end,replacement,reparsed,old_source=None):
    """Return a merged analysis and the formula starts that need projection."""
    delta=len(replacement.encode('utf-8'))-(end-start)
    shifted=[];unchanged=set()
    before=old_source.encode('utf-8') if old_source is not None else None
    after=new_source.encode('utf-8')
    # Delimiters outside a formula may move a binding into or out of its lexical
    # scope without changing the binding's text. Keep Typst's reparse invalidation
    # for these structural edits; ordinary prose still reuses the cached View.
    inside_formula=any(f['start']<start and end<f['end'] for f in previous.get('formulas',[]))
    context_changed=before is not None and not inside_formula and any(
        c in before[start:end]+replacement.encode('utf-8') for c in b'#[]{}')
    for old in previous.get('formulas',[]):
        formula=_shift_formula(old,start,end,delta,new_source)
        if formula is not None:
            shifted.append(formula)
            if before is not None and before[old['start']:old['end']]==after[formula['start']:formula['end']]:
                unchanged.add((formula['start'],formula['end']))
    by_range={(item['start'],item['end']):item for item in shifted}
    # A reparse can include a neighbouring unchanged let. Only changed binding
    # text/ranges invalidate its dependents; moving the same block is not an edit.
    old_bindings=set()
    for style in previous.get('styles',[]):
        if style['kind']!='let':continue
        bounds=_range(style['start'],style['end'],start,end,delta)
        old_bindings.add((*(bounds or (start,start)),style.get('text','')))
    new_bindings={(s['start'],s['end'],s.get('text','')) for s in syntax.get('styles',[]) if s['kind']=='let'}
    changed=old_bindings.symmetric_difference(new_bindings)
    invalidate_after=min((item[0] for item in changed),default=None)
    formulas=[];rebuild=[]
    for base in syntax.get('formulas',[]):
        cached=by_range.get((base['start'],base['end']))
        overlaps=base['start']<reparsed['end'] and reparsed['start']<base['end']
        # Typst may reparse a whole surrounding paragraph. A formula in that
        # paragraph still owns the same View when its actual source is unchanged.
        content_changed=(base['start'],base['end']) not in unchanged if before is not None else overlaps
        invalid=content_changed or (context_changed and overlaps) or (invalidate_after is not None and base['start']>=invalidate_after)
        if cached and not invalid and cached.get('display')==base.get('display'):
            formulas.append(cached)
        else:
            formulas.append(deepcopy(base));rebuild.append(base['start'])
    styles=deepcopy(syntax.get('styles',[]))
    for formula in formulas:
        if formula.get('editable') is False:styles.append({'kind':'formula_error','start':formula['start'],'end':formula['end'],'text':formula.get('reason','公式保留源码模式')})
    return {'formulas':formulas,'styles':styles},rebuild
