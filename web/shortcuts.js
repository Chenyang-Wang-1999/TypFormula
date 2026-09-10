import commands from '../config/editor-commands.json' with {type:'json'};
export {commands};
export function matches(event,binding){
  if(!binding||event.isComposing)return false;
  const tokens=binding.toLowerCase().split('+');const key=tokens.at(-1);
  return event.key.toLowerCase()===key&&event.ctrlKey===tokens.includes('ctrl')&&event.metaKey===tokens.includes('cmd')&&event.altKey===tokens.includes('alt')&&event.shiftKey===tokens.includes('shift');
}
export function installShortcuts({execute,inVSCode,configure}){
  let custom={};try{custom=JSON.parse(window.localStorage.getItem('visualTypst.shortcuts')||'{}');}catch{}
  const mac=/Mac/.test(navigator.platform);
  const get=c=>(custom[c.id]??(mac?c.mac||c.key:c.key))||'';
  for(const command of commands){const button=document.getElementById(command.id);if(button){button.title=command.title+(inVSCode?' · 可在 VS Code 键盘快捷方式中改键':get(command)?` (${get(command)})`:'');button.dataset.command=command.command;}}
  const listener=event=>{
    if(inVSCode||event.target.closest('dialog,input,select')||event.isComposing)return;
    const command=commands.find(c=>matches(event,get(c)));if(command){event.preventDefault();event.stopImmediatePropagation();execute(command.id);}
  };
  document.addEventListener('keydown',listener,true);
  return {dispose(){document.removeEventListener('keydown',listener,true);},configure(){
    if(inVSCode)return configure();
    const dialog=document.getElementById('shortcuts-dialog'),list=document.getElementById('shortcut-list');list.replaceChildren();
    for(const c of commands){const label=document.createElement('label'),input=document.createElement('input');label.textContent=c.title;input.value=get(c);input.dataset.action=c.id;input.placeholder='例如 ctrl+alt+i，留空取消';label.append(input);list.append(label);}
    document.getElementById('shortcut-form').onsubmit=event=>{event.preventDefault();const values=[...list.querySelectorAll('input')].map(i=>[i.dataset.action,i.value.trim().toLowerCase()]);const assigned=values.map(x=>x[1]).filter(Boolean);if(new Set(assigned).size!==assigned.length){document.getElementById('shortcut-error').textContent='快捷键重复，请修改后保存。';return;}custom=Object.fromEntries(values);window.localStorage.setItem('visualTypst.shortcuts',JSON.stringify(custom));dialog.close();};
    dialog.showModal();
  }};
}
