import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),extension=path.join(root,'extensions/vscode');
const actions=JSON.parse(await fs.readFile(path.join(root,'config/editor-commands.json'),'utf8'));
const manifest={name:'visual-typst',displayName:'Visual Typst',description:'Typst source editing with shared structural math, live page preview and configurable commands.',version:'0.2.1',publisher:'visual-typst-local',license:'GPL-2.0-or-later',engines:{vscode:'^1.90.0'},categories:['Programming Languages','Other'],main:'./src/extension.cjs',extensionKind:['workspace'],
  capabilities:{untrustedWorkspaces:{supported:'limited',description:'Source editing works; native compilation and package installation require a trusted workspace.'},virtualWorkspaces:false},
  contributes:{
    customEditors:[{viewType:'visualTypst.editor',displayName:'Visual Typst',selector:[{filenamePattern:'*.typ'}],priority:'option'}],
    commands:[{command:'visualTypst.open',title:'Visual Typst: 打开混合编辑器'},...actions.map(a=>({command:'visualTypst.'+a.command,title:'Visual Typst: '+a.title}))],
    keybindings:actions.filter(a=>a.key).map(a=>({command:'visualTypst.'+a.command,key:a.key,...a.mac?{mac:a.mac}:{},when:'activeCustomEditorId == visualTypst.editor'})),
    menus:{'explorer/context':[{command:'visualTypst.open',when:'resourceExtname == .typ',group:'navigation'}]},
    configuration:{title:'Visual Typst',properties:{'visualTypst.previewOnOpen':{type:'boolean',default:true,description:'打开混合编辑器时显示整页实时预览。'},'visualTypst.backendPath':{type:'string',default:'',scope:'machine',description:'可选：本机 visual-typst 后端路径，旁边须有匹配的 visual-typst-layout。默认使用扩展随附的后端。'}}}
  }
};
await fs.writeFile(path.join(extension,'package.json'),JSON.stringify(manifest,null,2)+'\n');
await fs.copyFile(path.join(root,'config/editor-commands.json'),path.join(extension,'commands.json'));
await fs.mkdir(path.join(extension,'media/fonts'),{recursive:true});await fs.mkdir(path.join(extension,'bin'),{recursive:true});
for(const file of ['index.html','editor.bundle.js','style.css','core.wasm','fonts/NewCMMath-Regular.otf','fonts/NewCM10-Italic.otf','fonts/NOTICE'])await fs.copyFile(path.join(root,'web',file),path.join(extension,'media',file));
const suffix=process.platform==='win32'?'.exe':'';
for(const [folder,name] of [['server','visual-typst'],['adapter','visual-typst-layout']])await fs.copyFile(path.join(root,'target',folder,'debug',name+suffix),path.join(extension,'bin',name+suffix));
if(process.platform!=='win32')for(const name of ['visual-typst','visual-typst-layout'])await fs.chmod(path.join(extension,'bin',name),0o755);
await fs.copyFile(path.join(root,'COPYING'),path.join(extension,'LICENSE'));
await fs.copyFile(path.join(root,'vendor/typst/NOTICE'),path.join(extension,'TYPST-NOTICE'));
if(process.argv.includes('--package')){
  await fs.mkdir(path.join(root,'dist'),{recursive:true});
  const target=process.platform+'-'+process.arch;
  const out=path.join(root,'dist',`visual-typst-${manifest.version}-${target}.vsix`);
  const result=spawnSync(process.execPath,[path.join(root,'node_modules/@vscode/vsce/vsce'),'package','--no-dependencies','--allow-missing-repository','--target',target,'--out',out],{cwd:extension,stdio:'inherit',windowsHide:true});
  if(result.error)throw result.error;if(result.status)process.exit(result.status);console.log(out);
}
