function minimalEdit(before,after){
  let from=0;while(from<before.length&&from<after.length&&before[from]===after[from])from++;
  // Never bisect a UTF-16 surrogate pair.
  if(from>0&&/[\uD800-\uDBFF]/.test(before[from-1]))from--;
  let to=before.length,end=after.length;
  while(to>from&&end>from&&before[to-1]===after[end-1]){to--;end--;}
  if(to>0&&to<before.length&&/[\uD800-\uDBFF]/.test(before[to-1])){to++;end++;}
  return {from,to,text:after.slice(from,end)};
}
function snapshot(document){return {source:document.getText(),version:document.version,dirty:document.isDirty,uri:document.uri.toString()};}
async function applyDocumentEdit(vscode,document,body){
  if(body.version!==document.version||body.base!==document.getText())return {...snapshot(document),accepted:false};
  if(typeof body.source!=='string'||body.source.length>4*1024*1024)throw new Error('文档大小超过限制');
  const change=minimalEdit(body.base,body.source);
  if(change.from!==change.to||change.text){const edit=new vscode.WorkspaceEdit();edit.replace(document.uri,new vscode.Range(document.positionAt(change.from),document.positionAt(change.to)),change.text);if(!await vscode.workspace.applyEdit(edit))return {...snapshot(document),accepted:false};}
  return {...snapshot(document),accepted:document.getText()===body.source};
}
module.exports={minimalEdit,snapshot,applyDocumentEdit};
