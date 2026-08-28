"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = require("vscode");
const http = require("http");
let server = null;
const DEFAULT_PORT = 8765;
function activate(context) {
    const statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    statusBarItem.text = '$(radio-tower) OpenCode-RS: Active';
    statusBarItem.tooltip = `OpenCode-RS Bridge running on http://127.0.0.1:${DEFAULT_PORT}`;
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);
    startBridgeServer(DEFAULT_PORT, statusBarItem);
    const restartCmd = vscode.commands.registerCommand('opencode-bridge.restart', () => {
        if (server) {
            server.close();
        }
        startBridgeServer(DEFAULT_PORT, statusBarItem);
        vscode.window.showInformationMessage(`OpenCode-RS Bridge zrestartowany na porcie ${DEFAULT_PORT}`);
    });
    context.subscriptions.push(restartCmd);
}
function startBridgeServer(port, statusBarItem) {
    server = http.createServer(async (req, res) => {
        // CORS headers
        res.setHeader('Access-Control-Allow-Origin', '*');
        res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
        res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization');
        if (req.method === 'OPTIONS') {
            res.writeHead(204);
            res.end();
            return;
        }
        const url = req.url || '/';
        // 1. Health & Status
        if (url === '/health' || url === '/') {
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
                status: 'ok',
                bridge: 'opencode-rs-bridge',
                editor: vscode.env.appName,
                version: vscode.version
            }));
            return;
        }
        // 2. List Models (/v1/models)
        if (url === '/v1/models' && req.method === 'GET') {
            try {
                let availableModels = [
                    { id: 'cursor-claude-3-7-sonnet', name: 'Claude 3.7 Sonnet (Cursor)', provider: 'cursor' },
                    { id: 'cursor-gpt-4o', name: 'GPT-4o (Cursor)', provider: 'cursor' },
                    { id: 'windsurf-cascade-sonnet', name: 'Claude 3.7 Sonnet (Windsurf)', provider: 'windsurf' },
                    { id: 'devin-cascade-sonnet', name: 'Claude 3.7 Sonnet (Devin)', provider: 'windsurf' },
                    { id: 'trae-seed-2.1-turbo', name: 'Seed 2.1 Turbo (Trae)', provider: 'trae' },
                    { id: 'trae-kimi-k2.5', name: 'Kimi K2.5 (Trae)', provider: 'trae' },
                    { id: 'trae-minimax-m3', name: 'MiniMax M3 (Trae)', provider: 'trae' },
                    { id: 'copilot-gpt-4o', name: 'GPT-4o (GitHub Copilot)', provider: 'copilot' },
                    { id: 'opencode-zen', name: 'OpenCode Zen (Claude Hybrid)', provider: 'opencode' },
                    { id: 'opencode-go', name: 'OpenCode Go (Fast)', provider: 'opencode' },
                    { id: 'antigravity-claude-3-7', name: 'Claude 3.7 (Antigravity)', provider: 'antigravity' },
                    { id: 'amazon-q', name: 'Amazon Q Developer', provider: 'amazon-q' },
                    { id: 'augment-code', name: 'Augment Code Agent', provider: 'augment' }
                ];
                // If vscode.lm is available, also query system chat models
                if (typeof vscode.lm !== 'undefined') {
                    try {
                        const lmModels = await vscode.lm.selectChatModels();
                        if (lmModels && lmModels.length > 0) {
                            for (const m of lmModels) {
                                availableModels.push({
                                    id: `vscode-lm-${m.id}`,
                                    name: `${m.name || m.id} (${m.vendor || 'VSCode LM'})`,
                                    provider: m.vendor || 'vscode'
                                });
                            }
                        }
                    }
                    catch (e) {
                        // ignore LM lookup errors
                    }
                }
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ object: 'list', data: availableModels }));
            }
            catch (err) {
                res.writeHead(500, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: err.message }));
            }
            return;
        }
        // 3. VS Code Extensions & Commands — Wariant A: proxy VS Code pluginów
        if (url === '/v1/extensions' && req.method === 'GET') {
            try {
                const exts = vscode.extensions.all.map(e => ({
                    id: e.id,
                    label: e.packageJSON?.displayName || e.id,
                    version: e.packageJSON?.version || "",
                    active: e.isActive,
                    contributes: Object.keys(e.packageJSON?.contributes || {})
                }));
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ object: 'list', data: exts }));
            }
            catch (err) {
                res.writeHead(500, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: err.message }));
            }
            return;
        }
        if (url === '/v1/commands' && req.method === 'GET') {
            try {
                const cmds = await vscode.commands.getCommands(true);
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ object: 'list', data: cmds }));
            }
            catch (err) {
                res.writeHead(500, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: err.message }));
            }
            return;
        }
        if (url === '/v1/commands/execute' && req.method === 'POST') {
            let body = '';
            req.on('data', chunk => { body += chunk; });
            req.on('end', async () => {
                try {
                    const { command, args } = JSON.parse(body);
                    const result = await vscode.commands.executeCommand(command, ...(args || []));
                    res.writeHead(200, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify({ success: true, result: result ?? null }));
                }
                catch (err) {
                    res.writeHead(500, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify({ success: false, error: err.message }));
                }
            });
            return;
        }
        // 4. Chat Completions (/v1/chat/completions)
        if (url === '/v1/chat/completions' && req.method === 'POST') {
            let body = '';
            req.on('data', chunk => { body += chunk; });
            req.on('end', async () => {
                try {
                    const parsed = JSON.parse(body);
                    const stream = parsed.stream !== false;
                    const model = parsed.model || 'default';
                    const messages = parsed.messages || [];
                    if (stream) {
                        res.writeHead(200, {
                            'Content-Type': 'text/event-stream',
                            'Cache-Control': 'no-cache',
                            'Connection': 'keep-alive'
                        });
                        await handleChatStream(model, messages, res);
                        res.write('data: [DONE]\n\n');
                        res.end();
                    }
                    else {
                        const reply = await handleChatSync(model, messages);
                        res.writeHead(200, { 'Content-Type': 'application/json' });
                        res.end(JSON.stringify({
                            id: `chatcmpl-${Date.now()}`,
                            object: 'chat.completion',
                            created: Math.floor(Date.now() / 1000),
                            model: model,
                            choices: [{
                                    index: 0,
                                    message: { role: 'assistant', content: reply },
                                    finish_reason: 'stop'
                                }]
                        }));
                    }
                }
                catch (err) {
                    if (!res.headersSent) {
                        res.writeHead(500, { 'Content-Type': 'application/json' });
                    }
                    res.end(JSON.stringify({ error: err.message }));
                }
            });
            return;
        }
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Endpoint not found' }));
    });
    server.listen(port, '127.0.0.1', () => {
        console.log(`[OpenCode-RS Bridge] Listening on http://127.0.0.1:${port}`);
    });
    server.on('error', (e) => {
        statusBarItem.text = '$(alert) OpenCode-RS: Error';
        statusBarItem.tooltip = `Bridge error: ${e.message}`;
    });
}
async function handleChatStream(model, messages, res) {
    // 1. Try VS Code Language Model API if present
    if (typeof vscode.lm !== 'undefined') {
        try {
            const models = await vscode.lm.selectChatModels();
            if (models && models.length > 0) {
                const targetModel = models[0];
                const vsMessages = [];
                for (const m of messages) {
                    if (m.role === 'user') {
                        vsMessages.push(vscode.LanguageModelChatMessage.User(m.content));
                    }
                    else if (m.role === 'assistant') {
                        vsMessages.push(vscode.LanguageModelChatMessage.Assistant(m.content));
                    }
                }
                const response = await targetModel.sendRequest(vsMessages, {}, new vscode.CancellationTokenSource().token);
                for await (const chunk of response.text) {
                    sendSSEChunk(res, chunk, model);
                }
                return;
            }
        }
        catch (e) {
            // fallback
        }
    }
    // 2. Fallback streaming response bridge
    const lastUserMessage = messages.filter(m => m.role === 'user').pop()?.content || '';
    const responseText = `[Odpowiedź z modelu ${model} przez mostek OpenCode-RS w ${vscode.env.appName}]\nOtrzymano zapytanie: "${lastUserMessage.slice(0, 80)}..."`;
    const chunks = responseText.split(' ');
    for (const chunk of chunks) {
        sendSSEChunk(res, chunk + ' ', model);
        await new Promise(r => setTimeout(r, 20));
    }
}
async function handleChatSync(model, messages) {
    let result = '';
    const fakeRes = {
        write: (data) => {
            if (data.startsWith('data: ') && !data.includes('[DONE]')) {
                try {
                    const json = JSON.parse(data.replace('data: ', '').trim());
                    result += json.choices?.[0]?.delta?.content || '';
                }
                catch { }
            }
        },
        end: () => { }
    };
    await handleChatStream(model, messages, fakeRes);
    return result;
}
function sendSSEChunk(res, text, model) {
    const payload = {
        id: `chatcmpl-${Date.now()}`,
        object: 'chat.completion.chunk',
        created: Math.floor(Date.now() / 1000),
        model: model,
        choices: [{
                index: 0,
                delta: { content: text },
                finish_reason: null
            }]
    };
    res.write(`data: ${JSON.stringify(payload)}\n\n`);
}
function deactivate() {
    if (server) {
        server.close();
        server = null;
    }
}
//# sourceMappingURL=extension.js.map