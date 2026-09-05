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
function tryListen(port, statusBarItem, attempt = 0) {
    if (attempt > 2) {
        statusBarItem.text = '$(alert) OpenCode-RS: No free port (8765-8767)';
        return;
    }
    startBridgeServer(port + attempt, statusBarItem);
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
        // 1b. Debug endpoint — raw vscode.lm.selectChatModels() output
        if (url === '/v1/lm/debug' && req.method === 'GET') {
            try {
                if (typeof vscode.lm === 'undefined') {
                    res.writeHead(503, { 'Content-Type': 'application/json' });
                    res.end(JSON.stringify({ error: 'vscode.lm undefined' }));
                    return;
                }
                const lmModels = await vscode.lm.selectChatModels();
                const raw = (lmModels || []).map((m) => ({
                    id: m.id,
                    name: m.name,
                    vendor: m.vendor,
                    family: m.family,
                    version: m.version,
                    maxInputTokens: m.maxInputTokens,
                    maxOutputTokens: m.maxOutputTokens,
                    capabilities: m.capabilities
                }));
                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ count: raw.length, models: raw }, null, 2));
            }
            catch (e) {
                res.writeHead(500, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: e.message }));
            }
            return;
        }
        // 2. List Models (/v1/models) — DYNAMIC, queries vscode.lm
        if (url === '/v1/models' && req.method === 'GET') {
            try {
                let availableModels = [];
                // Query vscode.lm dynamically — this returns ALL models available in the editor
                if (typeof vscode.lm !== 'undefined') {
                    try {
                        const lmModels = await vscode.lm.selectChatModels();
                        if (lmModels && lmModels.length > 0) {
                            for (const m of lmModels) {
                                const vendor = (m.vendor || '').toLowerCase();
                                const id = (m.id || '').toLowerCase();
                                const name = m.name || m.id || 'unknown';
                                // Map vendor to provider category
                                let provider = m.vendor || 'vscode';
                                if (vendor.includes('codeium') || vendor.includes('windsurf'))
                                    provider = 'windsurf';
                                else if (vendor.includes('cursor'))
                                    provider = 'cursor';
                                else if (vendor.includes('trae') || vendor.includes('bytedance'))
                                    provider = 'trae';
                                else if (vendor.includes('copilot') || vendor.includes('github'))
                                    provider = 'copilot';
                                else if (vendor.includes('google') || vendor.includes('antigravity') || id.includes('gemini'))
                                    provider = 'antigravity';
                                else if (vendor.includes('anthropic') || id.includes('claude'))
                                    provider = 'anthropic';
                                else if (vendor.includes('openai') || id.includes('gpt'))
                                    provider = 'openai';
                                else if (vendor.includes('amazon'))
                                    provider = 'amazon-q';
                                else if (vendor.includes('augment'))
                                    provider = 'augment';
                                availableModels.push({
                                    id: m.id,
                                    name: name,
                                    provider: provider,
                                    vendor: m.vendor || '',
                                    maxInputTokens: m.maxInputTokens || 0,
                                    maxOutputTokens: m.maxOutputTokens || 0
                                });
                            }
                        }
                    }
                    catch (e) {
                        // ignore LM lookup errors
                    }
                }
                // Fallback: if vscode.lm returned nothing, use minimal hardcoded list
                if (availableModels.length === 0) {
                    availableModels = [
                        { id: 'cursor-claude-3-7-sonnet', name: 'Claude 3.7 Sonnet (Cursor)', provider: 'cursor' },
                        { id: 'cursor-gpt-4o', name: 'GPT-4o (Cursor)', provider: 'cursor' },
                        { id: 'windsurf-cascade-sonnet', name: 'Claude 3.7 Sonnet (Windsurf)', provider: 'windsurf' },
                        { id: 'trae-seed-2.1-turbo', name: 'Seed 2.1 Turbo (Trae)', provider: 'trae' },
                        { id: 'antigravity-claude-3-7', name: 'Claude 3.7 (Antigravity)', provider: 'antigravity' },
                        { id: 'copilot-gpt-4o', name: 'GPT-4o (GitHub Copilot)', provider: 'copilot' },
                        { id: 'amazon-q', name: 'Amazon Q Developer', provider: 'amazon-q' },
                        { id: 'augment-code', name: 'Augment Code Agent', provider: 'augment' }
                    ];
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
        statusBarItem.text = `$(radio-tower) OpenCode-RS: Active :${port}`;
        statusBarItem.tooltip = `OpenCode-RS Bridge running on http://127.0.0.1:${port}`;
    });
    server.on('error', (e) => {
        if (e.code === 'EADDRINUSE' && port < 8767) {
            console.log(`[OpenCode-RS Bridge] Port ${port} in use, trying ${port + 1}`);
            setTimeout(() => startBridgeServer(port + 1, statusBarItem), 500);
        }
        else {
            statusBarItem.text = '$(alert) OpenCode-RS: Error';
            statusBarItem.tooltip = `Bridge error: ${e.message}`;
        }
    });
}
async function handleChatStream(model, messages, res) {
    if (typeof vscode.lm === 'undefined') {
        if (!res.headersSent) {
            res.writeHead(503, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Bridge: vscode.lm niedostepne. Zaloguj sie do edytora (Cursor/Windsurf/Trae/Antigravity).' }));
        }
        return false;
    }
    try {
        const allModels = await vscode.lm.selectChatModels();
        if (!allModels || allModels.length === 0) {
            if (!res.headersSent) {
                res.writeHead(503, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: 'Bridge: brak modeli w vscode.lm. Zaloguj sie do edytora.' }));
            }
            return false;
        }
        const ml = model.toLowerCase();
        const targetModel = allModels.find((m) => {
            const v = (m.vendor || '').toLowerCase();
            const id = (m.id || '').toLowerCase();
            if (ml.includes('cursor') && v.includes('cursor')) {
                return true;
            }
            if ((ml.includes('windsurf') || ml.includes('devin') || ml.includes('cascade')) && v.includes('codeium')) {
                return true;
            }
            if (ml.includes('trae') && (v.includes('trae') || v.includes('bytedance'))) {
                return true;
            }
            if (ml.includes('copilot') && (v.includes('copilot') || v.includes('github'))) {
                return true;
            }
            if ((ml.includes('antigravity') || ml.includes('gemini')) && (v.includes('google') || id.includes('gemini'))) {
                return true;
            }
            if (ml.includes('amazon') && v.includes('amazon')) {
                return true;
            }
            if (ml.includes('gpt') && (v.includes('openai') || id.includes('gpt'))) {
                return true;
            }
            if (ml.includes('claude') && (v.includes('anthropic') || id.includes('claude'))) {
                return true;
            }
            return false;
        }) || allModels[0];
        const vsMessages = messages
            .filter((m) => m.role === 'user' || m.role === 'assistant')
            .map((m) => m.role === 'user'
            ? vscode.LanguageModelChatMessage.User(m.content)
            : vscode.LanguageModelChatMessage.Assistant(m.content));
        const response = await targetModel.sendRequest(vsMessages, {}, new vscode.CancellationTokenSource().token);
        for await (const chunk of response.text) {
            sendSSEChunk(res, chunk, targetModel.id || model);
        }
        return true;
    }
    catch (e) {
        if (!res.headersSent) {
            res.writeHead(503, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'vscode.lm error: ' + (e && e.message ? e.message : String(e)) }));
        }
        return false;
    }
}
async function handleChatSync(model, messages) {
    let result = '';
    const fakeRes = {
        headersSent: false,
        writeHead: (_c, _h) => { },
        write: (data) => {
            if (data.startsWith('data: ') && !data.includes('[DONE]')) {
                try {
                    const json = JSON.parse(data.replace('data: ', '').trim());
                    result += json.choices?.[0]?.delta?.content || '';
                }
                catch (_) { }
            }
        },
        end: (_d) => { }
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