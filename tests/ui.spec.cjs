// Optional browser contract test; native/cofre validation is deliberately separate.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
(async()=>{
 const root=path.resolve(__dirname,'../ui');
 const server=http.createServer((req,res)=>{const file=path.join(root,req.url==='/'?'index.html':req.url);if(!file.startsWith(root+path.sep)){res.writeHead(403).end();return;}try{res.setHeader('Content-Type',file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html');res.end(fs.readFileSync(file));}catch{res.writeHead(404).end();}});
 await new Promise(r=>server.listen(0,'127.0.0.1',r));
 const browser=await chromium.launch({executablePath:process.env.CHROME_BIN,headless:true,chromiumSandbox:true});
 try{
  const page=await browser.newPage({viewport:{width:1200,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{window.testRequests=[];window.testState=JSON.parse(localStorage.getItem("fixture")||"null")||{created:true,unlocked:false,pin:true,keys:[{id:'key-one',name:'Minha chave'}],accounts:[],idleMinutes:0,version:'0.1.0'};window.__TAURI__={core:{invoke:async(cmd,args)=>{window.testRequests.push({cmd,args});if(cmd==='request'){if(args.op==='status')return structuredClone(window.testState);if(args.op==='unlock')throw Error('Credencial incorreta');if(args.op==='account.add'){window.testState.accounts.push({id:'demo',name:args.data.name});return structuredClone(window.testState);}}return null;}},event:{listen:async()=>()=>{}}};});
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.getByRole('button',{name:'PIN',exact:true}).click();
  await page.getByLabel('Dígito 1',{exact:true}).fill('1');await page.keyboard.type('23456');
  assert.deepEqual(await page.locator('.pin-inputs input').evaluateAll(n=>n.map(n=>n.value)),['1','2','3','4','5','6']);
  await page.getByRole('button',{name:'Desbloquear meu espaço'}).click();
  await page.getByRole('alert').filter({hasText:'Credencial incorreta'}).waitFor();
  assert.equal(await page.locator('.pin-inputs input').first().inputValue(),'');
  await page.screenshot({path:'/tmp/elo-pin.png'});
  await page.evaluate(()=>{window.testState.unlocked=true;window.testState.accounts=[{id:'a',name:'Pessoal'},{id:'b',name:'Trabalho'},{id:'c',name:'<img src=x onerror=alert(1)>'}];localStorage.setItem('fixture',JSON.stringify(window.testState));});
  await page.reload();await page.locator('#workspace:not([hidden])').waitFor();
  assert.equal(await page.locator('#account-list img').count(),0);
  await page.screenshot({path:'/tmp/elo-workspace.png'});
  await page.getByRole('button',{name:'Adicionar conta',exact:true}).click();
  await page.getByLabel('Nome da conta',{exact:true}).fill('Projeto');
  await page.getByRole('button',{name:'Conectar conta'}).click();
  await page.getByRole('button',{name:'Projeto',exact:true}).last().waitFor();
  assert.deepEqual(errors,[]);console.log('PASS: PIN, rejected credential, secret clearing, safe labels, adding account, dashboard.');
 }finally{await browser.close();server.close();}
})().catch(e=>{console.error(e);process.exit(1);});
