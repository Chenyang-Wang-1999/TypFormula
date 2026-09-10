import {normalizedMetrics} from './svg-metrics.js';

export class MacroWarmup {
  constructor({request,snapshot,apply,updated}) { Object.assign(this,{request,snapshot,apply,updated});this.records=new Map();this.revision=0;this.failures='[]'; }
  schedule(delay=350) {
    if(this.disposed)return;
    const path=this.snapshot().path;
    if(this.path!==path) {
      for(const record of this.records.values())for(const item of record.items)if(item.url)URL.revokeObjectURL(item.url);
      this.records.clear();this.pending=null;this.failures='[]';this.path=path;
    }
    this.revision++;clearTimeout(this.timer);this.timer=setTimeout(()=>this.run(),delay);
  }
  async run() {
    if(this.busy||this.disposed)return;
    const revision=this.revision,snapshot=this.snapshot();this.busy=true;
    try {
      const result=await this.request(snapshot);
      if(revision!==this.revision||this.disposed)return;
      const next=new Map();
      for(const record of result.results) {
        const items=(record.items||[]).map(item=>{normalizedMetrics(item);return {...item,status:'ready'};});
        next.set(record.key,{...record,items});
      }
      for(const record of this.records.values())for(const item of record.items)if(item.url)URL.revokeObjectURL(item.url);
      this.records=next;
      const failures=JSON.stringify(result.results.filter(r=>r.failed).map(r=>r.key).sort());
      if(failures!==this.failures) { this.pending=result.results.map(r=>[r.key,r.failed]);this.failures=failures; }
      this.flush();this.updated();
    } catch(error) { this.error=error.message; }
    finally { this.busy=false;if(revision!==this.revision)this.schedule(0); }
  }
  flush() { if(this.pending&&this.apply(this.pending))this.pending=null; }
  lookup(node) {
    const record=this.records.get(node.warmup_key);if(!record||record.failed)return null;
    const range=node.warmup_range?.join(':')||node.render_id?.split(':').slice(0,2).join(':');
    const item=record.items.find(item=>range&&item.id.startsWith(range+':'))||record.items.find(item=>item.template_source===node.text);
    if(item&&!item.url)item.url=URL.createObjectURL(new Blob([item.svg],{type:'image/svg+xml'}));
    return item;
  }
  dispose() { clearTimeout(this.timer);this.revision++;this.disposed=true;for(const record of this.records.values())for(const item of record.items)if(item.url)URL.revokeObjectURL(item.url);this.records.clear(); }
}
