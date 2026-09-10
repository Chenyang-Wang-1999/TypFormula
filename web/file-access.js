const typstTypes=[{description:'Typst 文档',accept:{'text/plain':['.typ']}}];

export function filePickerAvailable(scope=globalThis){
  return typeof scope.showOpenFilePicker==='function'&&typeof scope.showSaveFilePicker==='function';
}

export async function openTypstFile(scope=globalThis){
  if(filePickerAvailable(scope)){
    const [handle]=await scope.showOpenFilePicker({types:typstTypes,multiple:false,excludeAcceptAllOption:false});
    const file=await handle.getFile();
    return {name:file.name,source:await file.text(),handle,writable:true};
  }
  return new Promise((resolve,reject)=>{
    const input=document.createElement('input');input.type='file';input.accept='.typ,text/plain';
    input.onchange=async()=>{const file=input.files?.[0];if(file)resolve({name:file.name,source:await file.text(),handle:null,writable:false});else reject(new DOMException('未选择文件','AbortError'));};
    input.oncancel=()=>reject(new DOMException('未选择文件','AbortError'));
    input.click();
  });
}

export async function saveTypstAs(source,suggestedName='document.typ',scope=globalThis){
  if(filePickerAvailable(scope)){
    const handle=await scope.showSaveFilePicker({types:typstTypes,suggestedName});
    await writeTypstFile(handle,source);
    return {name:handle.name||suggestedName,handle,writable:true};
  }
  const url=URL.createObjectURL(new Blob([source],{type:'text/plain;charset=utf-8'}));
  try{const a=document.createElement('a');a.href=url;a.download=suggestedName;a.click();}
  finally{setTimeout(()=>URL.revokeObjectURL(url),1000);}
  return {name:suggestedName,handle:null,writable:false};
}

export async function writeTypstFile(handle,source){
  if(!handle)throw new Error('浏览器没有可写文件句柄，请使用“另存为”');
  const writable=await handle.createWritable();
  try{await writable.write(source);await writable.close();}
  catch(error){try{await writable.abort?.();}catch{}throw error;}
}
