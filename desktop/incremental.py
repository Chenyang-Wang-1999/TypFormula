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
    if isinstance(node.get('warmup_range'),list):
        shifted=_range(*node['warmup_range'],start,end,delta)
        if shifted and list(shifted)!=node['warmup_range']:
            result={**result,'warmup_range':list(shifted)}
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

def merge(previous,syntax,new_source,start,end,replacement,reparsed):
    """Return a merged analysis and the formula starts that need projection."""
    delta=len(replacement.encode('utf-8'))-(end-start)
    shifted=[]
    for old in previous.get('formulas',[]):
        formula=_shift_formula(old,start,end,delta,new_source)
        if formula is not None:shifted.append(formula)
    by_range={(item['start'],item['end']):item for item in shifted}
    old_let_changed=any(style['kind']=='let' and not (style['end']<=start or style['start']>=end) for style in previous.get('styles',[]))
    new_let_changed=any(style['kind']=='let' and not (style['end']<=reparsed['start'] or style['start']>=reparsed['end']) for style in syntax.get('styles',[]))
    invalidate_after=reparsed['start'] if old_let_changed or new_let_changed else None
    formulas=[];rebuild=[]
    for base in syntax.get('formulas',[]):
        cached=by_range.get((base['start'],base['end']))
        overlaps=base['start']<reparsed['end'] and reparsed['start']<base['end']
        invalid=overlaps or (invalidate_after is not None and base['start']>=invalidate_after)
        if cached and not invalid and cached.get('display')==base.get('display'):
            formulas.append(cached)
        else:
            formulas.append(deepcopy(base));rebuild.append(base['start'])
    styles=deepcopy(syntax.get('styles',[]))
    for formula in formulas:
        if formula.get('editable') is False:styles.append({'kind':'formula_error','start':formula['start'],'end':formula['end'],'text':formula.get('reason','公式保留源码模式')})
    return {'formulas':formulas,'styles':styles},rebuild
