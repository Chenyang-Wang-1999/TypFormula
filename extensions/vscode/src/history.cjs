// Undo/redo for a webview-based custom editor.
//
// VS Code's own undo command cannot serve a custom editor: it resolves
// `getFocusedCodeEditor() || getActiveCodeEditor()` and then calls
// `model.undo()` on that code editor, so with a webview focused it either does
// nothing or undoes an unrelated file. The host therefore keeps the history of
// every text this document has had and replays it through WorkspaceEdit, which
// keeps the TextDocument, its dirty flag and its version authoritative.
const LIMIT=200;
class EditHistory {
  constructor(text=''){this.past=[];this.future=[];this.current=text;}
  // Called for every text the document reports, whatever produced it.
  record(text){
    if(text===this.current)return;
    this.past.push(this.current);
    if(this.past.length>LIMIT)this.past.shift();
    this.future.length=0;
    this.current=text;
  }
  // The text a step in `direction` would restore, without applying it.
  peek(direction){const stack=direction==='redo'?this.future:this.past;return stack.length?stack[stack.length-1]:null;}
  // Only called once the edit that reaches that text was accepted.
  commit(direction){
    if(direction==='redo'){const next=this.future.pop();if(next===undefined)return null;this.past.push(this.current);this.current=next;}
    else{const previous=this.past.pop();if(previous===undefined)return null;this.future.push(this.current);this.current=previous;}
    return this.current;
  }
  get canUndo(){return this.past.length>0;}
  get canRedo(){return this.future.length>0;}
}
module.exports={EditHistory,LIMIT};
