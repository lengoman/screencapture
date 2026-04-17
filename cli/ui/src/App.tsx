import { useState, useEffect } from 'react';

interface AgentInfo {
  id: string;
  screens: number;
}

function App() {
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [timestamps, setTimestamps] = useState<Record<string, number>>({});
  const [loading, setLoading] = useState(true);

  const fetchAgents = async () => {
    try {
      const res = await fetch('/api/v1/agents');
      if (res.ok) {
        const data = await res.json();
        setAgents(data);
      }
    } catch (e) {
      console.error("Failed to fetch agents", e);
    } finally {
      if (loading) setLoading(false);
    }
  };

  useEffect(() => {
    fetchAgents();
    const interval = setInterval(fetchAgents, 3000);
    return () => clearInterval(interval);
  }, []);

  const triggerCapture = (agentId: string, screenId: number) => {
    const key = `${agentId}-${screenId}`;
    setTimestamps(prev => ({ ...prev, [key]: Date.now() }));
  };

  return (
    <div className="container">
      <header>
        <h1>Remote Capture Hub</h1>
        <div className="status-badge">
          {loading ? "Discovering..." : `${agents.length} Agents Connected`}
        </div>
      </header>

      <main className="grid">
        {!loading && agents.length === 0 && (
          <div className="empty-state">
            Wait for agents to connect via CLI...
          </div>
        )}
        
        {agents.flatMap((agent) => {
          // Fallback to 1 if screens is missing or 0 to ensure it always renders at least the default card
          const screenCount = Math.max(1, agent.screens || 1);
          return Array.from({ length: screenCount }).map((_, screenId) => {
            const key = `${agent.id}-${screenId}`;
            return (
              <div key={key} className="agent-card">
                <div className="card-header">
                  <h2>{agent.id} {screenCount > 1 && `(Display ${screenId})`}</h2>
                  <button 
                    onClick={() => triggerCapture(agent.id, screenId)}
                    className="capture-btn"
                  >
                    Pull Screenshot
                  </button>
                </div>
                
                <div className="image-container">
                  {timestamps[key] ? (
                    <img 
                      src={`/api/v1/capture/${agent.id}?screen=${screenId}&t=${timestamps[key]}`} 
                      alt={`Screenshot ${agent.id}`} 
                      className="screenshot"
                    />
                  ) : (
                    <div className="placeholder">
                      Click 'Pull Screenshot' to fetch this display
                    </div>
                  )}
                </div>
              </div>
            );
          });
        })}
      </main>
    </div>
  );
}

export default App;

