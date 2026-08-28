import * as vscode from 'vscode';
import * as http from 'http';

let server: http.Server | null = null;
const DEFAULT_PORT = 8765;

export function activate(context: vscode.ExtensionContext) {
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

function startBridgeServer(port: number, statusBarItem: vscode.StatusBarItem) {
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
                    { id: 'opencode-zen', name: 'OpenCode Zen (Claude 3.7 Hybrid & Thinking)', provider: 'opencode' },
                    { id: 'opencode-go', name: 'OpenCode Go (Fast)', provider: 'opencode' },
                    { id: 'opencode-flash', name: 'OpenCode Flash 3.7 (Instant)', provider: 'opencode' },
                    { id: 'opencode-pro', name: 'OpenCode Pro (Deep Reasoning)', provider: 'opencode' },
                    { id: 'opencode-claude-3-7-sonnet', name: 'Claude 3.7 Sonnet (OpenCode)', provider: 'opencode' },
                    { id: 'opencode-gpt-4o', name: 'GPT-4o Omnimodal (OpenCode)', provider: 'opencode' },
                    { id: 'opencode-deepseek-r1', name: 'DeepSeek R1 (OpenCode)', provider: 'opencode' },
                    { id: 'antigravity-claude-3-7', name: 'Claude 3.7 Sonnet Thinking (Antigravity)', provider: 'antigravity' },
                    { id: 'antigravity-claude-3-5-sonnet', name: 'Claude 3.5 Sonnet (Antigravity)', provider: 'antigravity' },
                    { id: 'antigravity-gemini-3-7-pro', name: 'Gemini 3.7 Pro (Antigravity)', provider: 'antigravity' },
                    { id: 'antigravity-gemini-3-7-flash', name: 'Gemini 3.7 Flash (Antigravity)', provider: 'antigravity' },
                    { id: 'antigravity-deepseek-r1', name: 'DeepSeek R1 (Antigravity)', provider: 'antigravity' },
                    { id: 'antigravity-gpt-4o', name: 'GPT-4o Omnimodal (Antigravity)', provider: 'antigravity' },
                    { id: 'commandcode-claude-3-7-sonnet', name: 'Claude 3.7 Sonnet Thinking (Command Code)', provider: 'commandcode' },
                    { id: 'commandcode-claude-3-5-sonnet', name: 'Claude 3.5 Sonnet v2 (Command Code)', provider: 'commandcode' },
                    { id: 'commandcode-gpt-4o', name: 'GPT-4o Omnimodal (Command Code)', provider: 'commandcode' },
                    { id: 'commandcode-deepseek-r1', name: 'DeepSeek R1 (Command Code)', provider: 'commandcode' },
                    { id: 'cursor-claude-3-7-sonnet', name: 'Claude 3.7 Sonnet (Cursor)', provider: 'cursor' },
                    { id: 'cursor-gpt-4o', name: 'GPT-4o (Cursor)', provider: 'cursor' },
                    { id: 'windsurf-cascade-sonnet', name: 'Claude 3.7 Sonnet (Windsurf)', provider: 'windsurf' },
                    { id: 'devin-cascade-sonnet', name: 'Claude 3.7 Sonnet (Devin)', provider: 'windsurf' },
                    { id: 'trae-seed-2.1-turbo', name: 'Seed 2.1 Turbo (Trae)', provider: 'trae' },
                    { id: 'trae-kimi-k2.5', name: 'Kimi K2.5 (Trae)', provider: 'trae' },
                    { id: 'trae-minimax-m3', name: 'MiniMax M3 (Trae)', provider: 'trae' },
                    { id: 'copilot-gpt-4o', name: 'GPT-4o (GitHub Copilot)', provider: 'copilot' },
                    { id: 'amazon-q', name: 'Amazon Q Developer', provider: 'amazon-q' },
                    { id: 'augment-code', name: 'Augment Code Agent', provider: 'augment' }
                ];

                // If vscode.lm is available, also query system chat models
                if (typeof (vscode as any).lm !== 'undefined') {
                    try {
                        const lmModels = await (vscode as any).lm.selectChatModels();
                        if (lmModels && lmModels.length > 0) {
                            for (const m of lmModels) {
                                availableModels.push({
                                    id: `vscode-lm-${m.id}`,
                                    name: `${m.name || m.id} (${m.vendor || 'VSCode LM'})`,
                                    provider: m.vendor || 'vscode'
                                });
                            }
                        }
                    } catch (e) {
                        // ignore LM lookup errors
                    }
                }

                res.writeHead(200, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ object: 'list', data: availableModels }));
            } catch (err: any) {
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
            } catch (err: any) {
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
            } catch (err: any) {
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
                } catch (err: any) {
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
                    } else {
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
                } catch (err: any) {
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

    server.on('error', (e: any) => {
        statusBarItem.text = '$(alert) OpenCode-RS: Error';
        statusBarItem.tooltip = `Bridge error: ${e.message}`;
    });
}

async function handleChatStream(model: string, messages: any[], res: http.ServerResponse): Promise<boolean> {
    if (typeof (vscode as any).lm === 'undefined') {
        if (!res.headersSent) {
            res.writeHead(503, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Bridge: vscode.lm niedostepne. Zaloguj sie do edytora (Cursor/Windsurf/Trae/Antigravity).' }));
        }
        return false;
    }
    try {
        const allModels = await (vscode as any).lm.selectChatModels();
        if (!allModels || allModels.length === 0) {
            if (!res.headersSent) {
                res.writeHead(503, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: 'Bridge: brak modeli w vscode.lm. Zaloguj sie do edytora.' }));
            }
            return false;
        }
        const ml = model.toLowerCase();
        const targetModel = allModels.find((m: any) => {
            const v = (m.vendor || '').toLowerCase();
            const id = (m.id || '').toLowerCase();
            if (ml.includes('cursor') && v.includes('cursor')) { return true; }
            if ((ml.includes('windsurf') || ml.includes('devin') || ml.includes('cascade')) && v.includes('codeium')) { return true; }
            if (ml.includes('trae') && (v.includes('trae') || v.includes('bytedance'))) { return true; }
            if (ml.includes('copilot') && (v.includes('copilot') || v.includes('github'))) { return true; }
            if ((ml.includes('antigravity') || ml.includes('gemini')) && (v.includes('google') || id.includes('gemini'))) { return true; }
            if (ml.includes('amazon') && v.includes('amazon')) { return true; }
            if (ml.includes('gpt') && (v.includes('openai') || id.includes('gpt'))) { return true; }
            if (ml.includes('claude') && (v.includes('anthropic') || id.includes('claude'))) { return true; }
            return false;
        }) || allModels[0];
        const vsMessages: any[] = messages
            .filter((m: any) => m.role === 'user' || m.role === 'assistant')
            .map((m: any) => m.role === 'user'
                ? (vscode as any).LanguageModelChatMessage.User(m.content)
                : (vscode as any).LanguageModelChatMessage.Assistant(m.content));
        const response = await targetModel.sendRequest(vsMessages, {}, new vscode.CancellationTokenSource().token);
        for await (const chunk of response.text) {
            sendSSEChunk(res, chunk, targetModel.id || model);
        }
        return true;
    } catch (e: any) {
        if (!res.headersSent) {
            res.writeHead(503, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'vscode.lm error: ' + (e && e.message ? e.message : String(e)) }));
        }
        return false;
    }
}
async function handleChatSync(model: string, messages: any[]): Promise<string> {
    let result = '';
    const fakeRes = {
        headersSent: false,
        writeHead: (_c: number, _h?: any) => {},
        write: (data: string) => {
            if (data.startsWith('data: ') && !data.includes('[DONE]')) {
                try {
                    const json = JSON.parse(data.replace('data: ', '').trim());
                    result += json.choices?.[0]?.delta?.content || '';
                } catch (_) {}
            }
        },
        end: (_d?: string) => {}
    } as any;

    await handleChatStream(model, messages, fakeRes);
    return result;
}

function sendSSEChunk(res: http.ServerResponse, text: string, model: string) {
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

export function deactivate() {
    if (server) {
        server.close();
        server = null;
    }
}
