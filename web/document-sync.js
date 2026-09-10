// One in-flight WorkspaceEdit. Never replace unacknowledged typing with an echo.
export class DocumentSync {
  constructor(initial,send,{replace,conflict,failed=()=>{},dirty=()=>{}}){
    this.confirmed=initial;this.local=initial.source;this.send=send;this.replace=replace;this.onConflict=conflict;this.onFailed=failed;this.dirty=dirty;this.running=null;this.conflict=null;this.failure=null;
  }
  edit(source){this.local=source;this.failure=null;this.dirty(true);this.pump();}
  pump(){
    // A failed submission is not a conflict: it says nothing about the remote
    // document, so it must not offer a merge direction. It stops the pump until
    // the next edit retries the same source.
    if(this.running||this.conflict||this.failure||this.local===this.confirmed.source)return;
    const base=this.confirmed, sent=this.local;
    this.running=this.send({version:base.version,base:base.source,source:sent}).then(reply=>{
      if(this.conflict)return;
      if(!reply.accepted){this.setConflict(reply);return;}
      this.confirmed=reply;
      if(reply.source!==sent)this.setConflict(reply);
      else this.dirty(reply.dirty);
    }).catch(error=>{this.failure=error;this.onFailed(error);}).finally(()=>{this.running=null;this.pump();});
  }
  remote(next){
    if(next.version<=this.confirmed.version)return;
    if(this.conflict){this.conflict.remote=next;this.onConflict(this.conflict);return;}
    if(this.running||this.local!==this.confirmed.source){this.setConflict(next);return;}
    this.confirmed=next;this.local=next.source;this.replace(next.source);this.dirty(next.dirty);
  }
  setConflict(remote,message='文档在另一处发生修改；当前输入已保留，请选择合并方向。'){
    this.conflict={remote,message};this.onConflict(this.conflict);
  }
  async resolve(keepLocal){
    if(this.running)await this.running;
    if(!this.conflict)return;
    this.confirmed=this.conflict.remote;this.conflict=null;this.onConflict(null);
    if(!keepLocal){this.local=this.confirmed.source;this.replace(this.local);this.dirty(this.confirmed.dirty);}
    this.pump();
  }
  async flush(){
    this.pump();while(this.running)await this.running;
    if(this.failure)throw this.failure;
    if(this.conflict)throw new Error('请先处理文档同步冲突；当前输入仍保留在编辑器中。');
  }
}
