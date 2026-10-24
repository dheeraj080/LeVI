import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RequestTab, Header, ResponseData } from "./types";
import "./App.css";

function App() {
  const [tabs, setTabs] = useState<RequestTab[]>([
    {
      id: "1",
      name: "New Request",
      method: "GET",
      url: "",
      headers: [],
      bodyType: "none",
      bodyContent: "",
      auth: { type: "none" },
    },
  ]);
  const [activeTabId, setActiveTabId] = useState("1");
  const [requestTab, setRequestTab] = useState<"params" | "headers" | "body" | "auth">("headers");
  const [responseTab, setResponseTab] = useState<"body" | "headers">("body");
  const [loading, setLoading] = useState(false);

  const activeTab = tabs.find((t) => t.id === activeTabId)!;

  const updateActiveTab = (updates: Partial<RequestTab>) => {
    setTabs(tabs.map((t) => (t.id === activeTabId ? { ...t, ...updates } : t)));
  };

  const addHeader = () => {
    updateActiveTab({
      headers: [...activeTab.headers, { key: "", value: "", enabled: true }],
    });
  };

  const updateHeader = (index: number, updates: Partial<Header>) => {
    const newHeaders = [...activeTab.headers];
    newHeaders[index] = { ...newHeaders[index], ...updates };
    updateActiveTab({ headers: newHeaders });
  };

  const removeHeader = (index: number) => {
    updateActiveTab({ headers: activeTab.headers.filter((_, i) => i !== index) });
  };

  const sendRequest = async () => {
    setLoading(true);
    try {
      const headers = activeTab.headers
        .filter((h) => h.enabled && h.key)
        .map((h) => [h.key, h.value]);

      const authData: any = {};
      if (activeTab.auth.type === "bearer") {
        authData.token = activeTab.auth.bearerToken;
      } else if (activeTab.auth.type === "basic") {
        authData.username = activeTab.auth.basicUsername;
        authData.password = activeTab.auth.basicPassword;
      } else if (activeTab.auth.type === "apikey") {
        authData.name = activeTab.auth.apiKeyName;
        authData.value = activeTab.auth.apiKeyValue;
        authData.in = activeTab.auth.apiKeyIn;
      }

      const res = await invoke<ResponseData>("send_request", {
        cmd: {
          method: activeTab.method,
          url: activeTab.url,
          headers,
          body_type: activeTab.bodyType,
          body_content: activeTab.bodyContent,
          auth_type: activeTab.auth.type === "none" ? null : activeTab.auth.type,
          auth_data: activeTab.auth.type === "none" ? null : authData,
        },
      });

      updateActiveTab({ response: res });
    } catch (e) {
      updateActiveTab({
        response: {
          status: 0,
          headers: [],
          body: String(e),
          timings: {},
        },
      });
    } finally {
      setLoading(false);
    }
  };

  const newTab = () => {
    const id = String(Date.now());
    setTabs([
      ...tabs,
      {
        id,
        name: "New Request",
        method: "GET",
        url: "",
        headers: [],
        bodyType: "none",
        bodyContent: "",
        auth: { type: "none" },
      },
    ]);
    setActiveTabId(id);
  };

  const closeTab = (id: string) => {
    if (tabs.length === 1) return;
    const newTabs = tabs.filter((t) => t.id !== id);
    setTabs(newTabs);
    if (activeTabId === id) {
      setActiveTabId(newTabs[0].id);
    }
  };

  return (
    <div className="app">
      <div className="sidebar">
        <div className="sidebar-header">
          <h2>Collections</h2>
          <button>+ New</button>
        </div>
        <div className="collections">
          <div style={{ padding: "5px", color: "#858585", fontSize: "12px" }}>
            No collections yet
          </div>
        </div>
      </div>
      <div className="main">
        <div className="tabs">
          {tabs.map((tab) => (
            <div
              key={tab.id}
              className={`tab ${tab.id === activeTabId ? "active" : ""}`}
              onClick={() => setActiveTabId(tab.id)}
            >
              <span>{tab.method} {tab.name}</span>
              {tabs.length > 1 && (
                <button className="tab-close" onClick={(e) => { e.stopPropagation(); closeTab(tab.id); }}>×</button>
              )}
            </div>
          ))}
          <button onClick={newTab} style={{ background: "none", border: "none", color: "#cccccc", padding: "8px 12px", cursor: "pointer" }}>+</button>
        </div>
        <div className="request-area">
          <div className="request-line">
            <select value={activeTab.method} onChange={(e) => updateActiveTab({ method: e.target.value })}>
              <option>GET</option>
              <option>POST</option>
              <option>PUT</option>
              <option>PATCH</option>
              <option>DELETE</option>
              <option>HEAD</option>
              <option>OPTIONS</option>
            </select>
            <input
              value={activeTab.url}
              onChange={(e) => updateActiveTab({ url: e.target.value })}
              placeholder="Enter request URL"
            />
            <button onClick={sendRequest} disabled={loading}>
              {loading ? "Sending..." : "Send"}
            </button>
          </div>
          <div className="request-tabs">
            <button className={`request-tab ${requestTab === "params" ? "active" : ""}`} onClick={() => setRequestTab("params")}>Params</button>
            <button className={`request-tab ${requestTab === "headers" ? "active" : ""}`} onClick={() => setRequestTab("headers")}>Headers</button>
            <button className={`request-tab ${requestTab === "body" ? "active" : ""}`} onClick={() => setRequestTab("body")}>Body</button>
            <button className={`request-tab ${requestTab === "auth" ? "active" : ""}`} onClick={() => setRequestTab("auth")}>Auth</button>
          </div>
          <div className="request-content">
            {requestTab === "headers" && (
              <div className="headers-editor">
                {activeTab.headers.map((header, index) => (
                  <div key={index} className="header-row">
                    <input type="checkbox" checked={header.enabled} onChange={(e) => updateHeader(index, { enabled: e.target.checked })} />
                    <input type="text" placeholder="Key" value={header.key} onChange={(e) => updateHeader(index, { key: e.target.value })} />
                    <input type="text" placeholder="Value" value={header.value} onChange={(e) => updateHeader(index, { value: e.target.value })} />
                    <button onClick={() => removeHeader(index)}>×</button>
                  </div>
                ))}
                <button onClick={addHeader} style={{ alignSelf: "flex-start", background: "none", border: "none", color: "#0e639c", cursor: "pointer" }}>+ Add Header</button>
              </div>
            )}
            {requestTab === "body" && (
              <div>
                <select value={activeTab.bodyType} onChange={(e) => updateActiveTab({ bodyType: e.target.value as any })} style={{ background: "#3c3c3c", color: "#cccccc", border: "1px solid #3c3c3c", padding: "4px", marginBottom: "10px" }}>
                  <option value="none">none</option>
                  <option value="json">json</option>
                  <option value="raw">raw</option>
                  <option value="form">form</option>
                </select>
                {activeTab.bodyType !== "none" && (
                  <div className="body-editor">
                    <textarea
                      value={activeTab.bodyContent}
                      onChange={(e) => updateActiveTab({ bodyContent: e.target.value })}
                      placeholder={activeTab.bodyType === "json" ? '{"key": "value"}' : "Enter body content"}
                    />
                  </div>
                )}
              </div>
            )}
            {requestTab === "auth" && (
              <div className="auth-editor">
                <select value={activeTab.auth.type} onChange={(e) => updateActiveTab({ auth: { type: e.target.value as any } })}>
                  <option value="none">No Auth</option>
                  <option value="bearer">Bearer Token</option>
                  <option value="basic">Basic Auth</option>
                  <option value="apikey">API Key</option>
                </select>
                {activeTab.auth.type === "bearer" && (
                  <input
                    type="text"
                    placeholder="Token"
                    value={activeTab.auth.bearerToken || ""}
                    onChange={(e) => updateActiveTab({ auth: { ...activeTab.auth, bearerToken: e.target.value } })}
                  />
                )}
                {activeTab.auth.type === "basic" && (
                  <>
                    <input
                      type="text"
                      placeholder="Username"
                      value={activeTab.auth.basicUsername || ""}
                      onChange={(e) => updateActiveTab({ auth: { ...activeTab.auth, basicUsername: e.target.value } })}
                    />
                    <input
                      type="password"
                      placeholder="Password"
                      value={activeTab.auth.basicPassword || ""}
                      onChange={(e) => updateActiveTab({ auth: { ...activeTab.auth, basicPassword: e.target.value } })}
                    />
                  </>
                )}
                {activeTab.auth.type === "apikey" && (
                  <>
                    <input
                      type="text"
                      placeholder="Key"
                      value={activeTab.auth.apiKeyName || ""}
                      onChange={(e) => updateActiveTab({ auth: { ...activeTab.auth, apiKeyName: e.target.value } })}
                    />
                    <input
                      type="text"
                      placeholder="Value"
                      value={activeTab.auth.apiKeyValue || ""}
                      onChange={(e) => updateActiveTab({ auth: { ...activeTab.auth, apiKeyValue: e.target.value } })}
                    />
                    <select value={activeTab.auth.apiKeyIn || "header"} onChange={(e) => updateActiveTab({ auth: { ...activeTab.auth, apiKeyIn: e.target.value as any } })}>
                      <option value="header">Header</option>
                      <option value="query">Query Params</option>
                    </select>
                  </>
                )}
              </div>
            )}
            {requestTab === "params" && (
              <div style={{ color: "#858585", fontSize: "12px" }}>Query parameters coming soon</div>
            )}
          </div>
          {activeTab.response && (
            <div className="response-area">
              <div className="response-header">
                <span>Status: {activeTab.response.status}</span>
              </div>
              <div className="response-tabs">
                <button className={`response-tab ${responseTab === "body" ? "active" : ""}`} onClick={() => setResponseTab("body")}>Body</button>
                <button className={`response-tab ${responseTab === "headers" ? "active" : ""}`} onClick={() => setResponseTab("headers")}>Headers</button>
              </div>
              <div className="response-content">
                {responseTab === "body" && (
                  <pre>{activeTab.response.body}</pre>
                )}
                {responseTab === "headers" && (
                  <pre>{JSON.stringify(activeTab.response.headers, null, 2)}</pre>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
