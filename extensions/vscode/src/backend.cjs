const {spawn}=require('node:child_process');
const readline=require('node:readline');
class Backend {
  constructor(executable,root,env,log){Object.assign(this,{executable,root,env,log});this.next=0;this.pending=new Map();this.tail=Promise.resolve();}
  start(){
    if(this.child)return;
    const child=spawn(this.executable,['--stdio',this.root],{cwd:this.root,env:this.env,windowsHide:true,stdio:['pipe','pipe','pipe']});this.child=child;
    const lines=readline.createInterface({input:child.stdout});
    lines.on('line',line=>{try{if(line.length>64*1024*1024)throw new Error('后端响应过大');const message=JSON.parse(line),task=this.pending.get(message.id);if(!task)return;this.pending.delete(message.id);clearTimeout(task.timer);if(message.error)task.reject(new Error(message.error));else task.resolve(message.result);}catch(error){this.stop(error);}});
    child.stderr.on('data',data=>this.log?.appendLine(data.toString()));
    child.on('error',error=>this.stop(error));child.on('exit',()=>{if(this.child===child)this.stop(new Error('Typst 后端已退出'));});
  }
  request(route,body){
    const task=()=>new Promise((resolve,reject)=>{
      if(this.disposed){reject(new Error('编辑器已关闭'));return;}
      this.start();const id=++this.next,timer=setTimeout(()=>this.stop(new Error('Typst 后端请求超时')),55000);
      this.pending.set(id,{resolve,reject,timer});this.child.stdin.write(JSON.stringify({id,route,body})+'\n',error=>{if(error)this.stop(error);});
    });
    const result=this.tail.then(task);this.tail=result.catch(()=>{});return result;
  }
  stop(error=new Error('编辑器已关闭')){const child=this.child;this.child=null;child?.kill();for(const task of this.pending.values()){clearTimeout(task.timer);task.reject(error);}this.pending.clear();}
  dispose(){this.disposed=true;this.stop();}
}
module.exports={Backend};
