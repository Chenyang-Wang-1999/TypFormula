const vscode=require('vscode');
const fs=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const {Backend}=require('./backend.cjs');
const {snapshot,applyDocumentEdit}=require('./document.cjs');
const {EditHistory}=require('./history.cjs');
const commands=require('../commands.json');
const VIEW='visualTypst.editor';
const pos=p=>({line:p.line,character:p.character});
const range=r=>({start:pos(r.start),end:pos(r.end)});
const textEdit=e=>({range:range(e.range),newText:e.newText});
function escapeHtml(value){return value.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}

class Provider {
  constructor(context){this.context=context;this.sessions=new Set();this.backends=new Map();this.edits=new Map();this.applying=new Map();this.histories=new Map();this.log=vscode.window.createOutputChannel('Visual Typst');context.subscriptions.push(this.log);}
  async backend(root){
    if(!vscode.workspace.isTrusted)throw new Error('请信任此工作区后使用本地 Typst 编译和包安装。');
    if(!this.backends.has(root)){
      const configuration=vscode.workspace.getConfiguration('visualTypst');
      const executable=configuration.get('backendPath')||path.join(this.context.extensionPath,'bin',process.platform==='win32'?'visual-typst.exe':'visual-typst');
      await fs.access(executable).catch(()=>{throw new Error('未找到此平台的 Typst 后端，请安装匹配平台的 VSIX，或配置 visualTypst.backendPath。');});
      const env={...process.env};const tinymist=vscode.extensions.getExtension('myriad-dreamin.tinymist');
      if(tinymist){const bin=path.join(tinymist.extensionPath,'out',process.platform==='win32'?'tinymist.exe':'tinymist');if(await fs.access(bin).then(()=>true,()=>false))env.TINYMIST_BIN=bin;}
      this.backends.set(root,new Backend(executable,root,env,this.log));
    }
    return this.backends.get(root);
  }
  async resolveCustomTextEditor(document,panel){
    if(document.uri.scheme!=='file'&&document.uri.scheme!=='untitled')throw new Error('此版本支持本地或远程扩展宿主中的文件及未命名文档。');
    const workspace=vscode.workspace.getWorkspaceFolder(document.uri);
    const root=workspace?.uri.fsPath||(document.uri.scheme==='file'?path.dirname(document.uri.fsPath):vscode.workspace.workspaceFolders?.[0]?.uri.fsPath);
    const session={document,panel,root,disposed:false};this.sessions.add(session);if(panel.active)this.active=session;
    const media=vscode.Uri.joinPath(this.context.extensionUri,'media');
    panel.webview.options={enableScripts:true,localResourceRoots:[media]};
    const disposables=[];
    disposables.push(panel.webview.onDidReceiveMessage(message=>this.message(session,message)));
    disposables.push(panel.onDidChangeViewState(()=>{if(panel.active)this.active=session;}));
    disposables.push(vscode.workspace.onDidChangeTextDocument(event=>{
      if(event.document.uri.toString()!==document.uri.toString()||!event.contentChanges.length)return;
      this.history(session).record(document.getText());
      if(this.applying.get(document.uri.toString())!==session)panel.webview.postMessage({type:'document',...snapshot(document)});
    }));
    disposables.push(vscode.workspace.onDidSaveTextDocument(doc=>{if(doc===document)panel.webview.postMessage({type:'saved',...snapshot(doc)});}));
    disposables.push(vscode.languages.onDidChangeDiagnostics(event=>{if(event.uris.some(uri=>uri.toString()===document.uri.toString()))panel.webview.postMessage({type:'diagnosticsChanged'});}));
    disposables.push(vscode.workspace.onDidChangeConfiguration(event=>{if(event.affectsConfiguration('editor.fontSize')||event.affectsConfiguration('editor.fontFamily')||event.affectsConfiguration('visualTypst'))panel.webview.postMessage({type:'settings',settings:this.settings(document)});}));
    panel.onDidDispose(()=>{session.disposed=true;disposables.forEach(d=>d.dispose());this.sessions.delete(session);if(this.active===session)this.active=null;if(root&&![...this.sessions].some(s=>s.root===root)){this.backends.get(root)?.dispose();this.backends.delete(root);}});
    let html=await fs.readFile(path.join(this.context.extensionPath,'media/index.html'),'utf8');
    const base=panel.webview.asWebviewUri(media).toString()+'/',nonce=crypto.randomBytes(16).toString('hex');
    html=html.replace('<head>',`<head><meta name="asset-base" content="${escapeHtml(base)}"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-${nonce}' 'wasm-unsafe-eval'; style-src ${panel.webview.cspSource} 'unsafe-inline'; font-src ${panel.webview.cspSource}; img-src ${panel.webview.cspSource} blob: data:; connect-src ${panel.webview.cspSource};">`)
      .replace(/(href|src)="\/([^" ]+)"/g,(_,attribute,file)=>`${attribute}="${escapeHtml(base+file)}"`).replace('<script type="module"',`<script nonce="${nonce}" type="module"`);
    panel.webview.html=html;
  }
  settings(document){const editor=vscode.workspace.getConfiguration('editor',document.uri),config=vscode.workspace.getConfiguration('visualTypst',document.uri);return {fontSize:editor.get('fontSize',16),fontFamily:editor.get('fontFamily','Consolas, monospace'),previewOnOpen:config.get('previewOnOpen',true)};}
  history(session){const key=session.document.uri.toString();if(!this.histories.has(key))this.histories.set(key,new EditHistory(session.document.getText()));return this.histories.get(key);}
  filePath(session){if(!session.root)throw new Error('预览需要打开一个工作区或先保存文件。');const relative=session.document.uri.scheme==='untitled'?'untitled.typ':path.relative(session.root,session.document.uri.fsPath).split(path.sep).join('/');if(relative.startsWith('../')||path.isAbsolute(relative))throw new Error('文件不在工作区内');return relative;}
  async message(session,message){
    if(message?.type==='focus'){this.active=session;return;}
    if(message?.type!=='request'||!Number.isSafeInteger(message.id))return;
    try{const result=await this.route(session,message.route,message.body||{});if(!session.disposed)session.panel.webview.postMessage({type:'reply',id:message.id,result});}
    catch(error){this.log.appendLine(error.stack||error.message);if(!session.disposed)session.panel.webview.postMessage({type:'reply',id:message.id,error:error.message});}
  }
  async route(session,route,body){
    const {document}=session;
    if(route==='/api/document')return {...snapshot(document),path:session.root?this.filePath(session):'untitled.typ',settings:this.settings(document)};
    if(route==='/api/edit'){
      const key=document.uri.toString(),previous=this.edits.get(key)||Promise.resolve();
      const edit=previous.catch(()=>{}).then(async()=>{this.applying.set(key,session);try{return await applyDocumentEdit(vscode,document,body);}finally{this.applying.delete(key);}});
      this.edits.set(key,edit);try{return await edit;}finally{if(this.edits.get(key)===edit)this.edits.delete(key);}
    }
    if(route==='/api/save'){if(!await document.save())throw new Error('保存已取消');return snapshot(document);}
    if(route==='/api/host-command')return this.hostCommand(session,body.command);
    if(route==='/api/copy'){
      const destination=await vscode.window.showSaveDialog({defaultUri:document.uri.scheme==='file'?document.uri:undefined,filters:{Typst:['typ']}});
      if(destination)await vscode.workspace.fs.writeFile(destination,Buffer.from(document.getText()));return {saved:!!destination};
    }
    if(route==='/api/new'){const doc=await vscode.workspace.openTextDocument({language:'typst',content:''});await vscode.commands.executeCommand('vscode.openWith',doc.uri,VIEW);return {};}
    if(route==='/api/reveal'){const uri=vscode.Uri.parse(body.uri);if(!['file','untitled'].includes(uri.scheme))throw new Error('不支持的定义目标');await vscode.window.showTextDocument(uri,{selection:new vscode.Range(body.position.line,body.position.character,body.position.line,body.position.character)});return {};}
    if(route==='/api/lsp')return this.language(session,body);
    if(route==='/api/status'){
      if(!session.root||!vscode.workspace.isTrusted)return {available:false,attachments:false};
      const status=await (await this.backend(session.root)).request(route,{});return {...status,available:status.available||!!vscode.extensions.getExtension('myriad-dreamin.tinymist')};
    }
    if(!['/api/render','/api/preview','/api/attachments','/api/completion','/api/packages'].includes(route))throw new Error('未允许的扩展请求');
    const backend=await this.backend(session.root||(()=>{throw new Error('请先打开工作区');})());
    if(route==='/api/render'||route==='/api/preview'){
      if(body.source!==document.getText())throw new Error('文档已变化，忽略过期编译请求');
      const overlays={};for(const doc of vscode.workspace.textDocuments){if(doc.uri.scheme!=='file')continue;const name=path.relative(session.root,doc.uri.fsPath).split(path.sep).join('/');if(!name.startsWith('../')&&!path.isAbsolute(name))overlays[name]=doc.getText();}
      return backend.request(route,{...body,path:this.filePath(session),source:document.getText(),overlays,preview:route==='/api/preview'});
    }
    return backend.request(route,route==='/api/attachments'?{...body,path:this.filePath(session)}:body);
  }
  async language(session,body){
    const extension=vscode.extensions.getExtension('myriad-dreamin.tinymist');
    if(!extension)return (await this.backend(session.root)).request('/api/lsp',{...body,path:this.filePath(session),source:session.document.getText()});
    await extension.activate();const doc=session.document;
    if(body.source!==doc.getText())throw new Error('文档尚未同步，请重试');
    const at=new vscode.Position(body.position?.line||0,body.position?.character||0);let result=null;
    if(body.method==='completion'){
      const list=await vscode.commands.executeCommand('vscode.executeCompletionItemProvider',doc.uri,at);
      result={items:(list?.items||[]).map(item=>({label:typeof item.label==='string'?item.label:item.label.label,detail:item.detail,insertText:typeof item.insertText==='string'?item.insertText:item.insertText?.value,insertTextFormat:item.insertText?.value?2:1,textEdit:item.textEdit?textEdit(item.textEdit):item.range?{range:range(item.range.replacing||item.range),newText:typeof item.insertText==='string'?item.insertText:item.insertText?.value||(typeof item.label==='string'?item.label:item.label.label)}:undefined,additionalTextEdits:item.additionalTextEdits?.map(textEdit)}))};
    }else if(body.method==='hover'){const values=await vscode.commands.executeCommand('vscode.executeHoverProvider',doc.uri,at);if(values?.length)result={contents:values.flatMap(h=>h.contents.map(c=>({value:typeof c==='string'?c:c.value})))};}
    else if(body.method==='definition'){const values=await vscode.commands.executeCommand('vscode.executeDefinitionProvider',doc.uri,at);result=values?.map(item=>item.targetUri?{targetUri:item.targetUri.toString(),targetSelectionRange:range(item.targetSelectionRange)}:{uri:item.uri.toString(),range:range(item.range)});}
    else if(body.method==='formatting'){const values=await vscode.commands.executeCommand('vscode.executeFormatDocumentProvider',doc.uri,{tabSize:2,insertSpaces:true});result=values?.map(textEdit)||[];}
    const diagnostics=vscode.languages.getDiagnostics(doc.uri).map(d=>({range:range(d.range),severity:d.severity+1,message:d.message}));
    return {result,diagnostics,uri:doc.uri.toString(),version:doc.version};
  }
  // The workbench undo command targets the focused code editor, not this
  // custom editor's TextDocument, so the host replays its own document history.
  async editHistory(session,direction){
    const history=this.history(session),target=history.peek(direction);
    if(target===null)return {changed:false,canUndo:history.canUndo,canRedo:history.canRedo};
    const document=session.document;
    const applied=await applyDocumentEdit(vscode,document,{version:document.version,base:document.getText(),source:target});
    if(applied.accepted)history.commit(direction);
    return {changed:applied.accepted,canUndo:history.canUndo,canRedo:history.canRedo};
  }
  async hostCommand(session,command){
    if(command==='configureShortcuts'){await vscode.commands.executeCommand('workbench.action.openGlobalKeybindings','@ext:visual-typst-local.visual-typst');return {};}
    if(command==='undo'||command==='redo')return this.editHistory(session,command);
    throw new Error('未知宿主命令');
  }
  dispose(){for(const backend of this.backends.values())backend.dispose();this.backends.clear();}
}
function activate(context){
  const provider=new Provider(context);context.subscriptions.push(provider);
  context.subscriptions.push(vscode.window.registerCustomEditorProvider(VIEW,provider,{webviewOptions:{retainContextWhenHidden:true},supportsMultipleEditorsPerDocument:true}));
  for(const action of commands)context.subscriptions.push(vscode.commands.registerCommand('visualTypst.'+action.command,()=>{
    const session=provider.active;if(!session)return;
    // Undo and redo post to the webview like any other toolbar command; the
    // webview asks the host, and the host replays this document's history.
    session.panel.webview.postMessage({type:'command',id:action.id});
  }));
  context.subscriptions.push(vscode.commands.registerCommand('visualTypst.open',async(uri)=>{uri=uri||vscode.window.activeTextEditor?.document.uri;if(uri)await vscode.commands.executeCommand('vscode.openWith',uri,VIEW);}));
}
module.exports={activate,Provider};
