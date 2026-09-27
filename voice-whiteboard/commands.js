window.applyBoardUpdate({id:'test-dots-002',run(board){
  const state=board.getState();
  state.nodes=state.nodes.filter(n=>!n.id.startsWith('test-dot-')||Number(n.id.slice(9))<50);
  board.setState(state);
}});
if(!document.getElementById('red-test-dots')){
  const style=document.createElement('style');
  style.id='red-test-dots';
  style.textContent='.node.dot { background:#ff596b; box-shadow:0 0 18px #ff596b66; }';
  document.head.appendChild(style);
}
