const host = typeof acquireVsCodeApi === 'function' ? acquireVsCodeApi() : null;
export const inVSCode = !!host;
const listeners=new Set(), pending=new Map();let sequence=0;
export function onHostMessage(callback){listeners.add(callback);return ()=>listeners.delete(callback);}
if(host)window.addEventListener('message',event=>{
  const msg=event.data;
  if(msg?.type==='reply'){
    const task=pending.get(msg.id);if(!task)return;pending.delete(msg.id);clearTimeout(task.timer);
    if(msg.error)task.reject(new Error(msg.error));else task.resolve(msg.result);
  }else for(const listener of listeners)listener(msg);
});
export function post(message){host?.postMessage(message);}
export function assetUrl(name){return inVSCode?new URL(name,document.querySelector('meta[name="asset-base"]').content).href:'/'+name;}
export async function request(route,body){
  if(!host){
    const response=await fetch(route,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
    const result=await response.json();if(!response.ok||result.error)throw new Error(result.error||`HTTP ${response.status}`);return result;
  }
  const id=++sequence;
  return new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>{pending.delete(id);reject(new Error('扩展请求超时：'+route));},65000);
    pending.set(id,{resolve,reject,timer});host.postMessage({type:'request',id,route,body});
  });
}
