import { useState, useEffect } from 'react';

function App() {
  const [agents, setAgents] = useState<string[]>([]);
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

  const triggerCapture = (agentId: string) => {
    setTimestamps(prev => ({ ...prev, [agentId]: Date.now() }));
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
        
        {agents.map((id) => (
          <div key={id} className="agent-card">
            <div className="card-header">
              <h2>{id}</h2>
              <button 
                onClick={() => triggerCapture(id)}
                className="capture-btn"
              >
                Pull Screenshot
              </button>
            </div>
            
            <div className="image-container">
              {timestamps[id] ? (
                <img 
                  src={`/api/v1/capture/${id}?t=${timestamps[id]}`} 
                  alt={`Screenshot ${id}`} 
                  className="screenshot"
                />
              ) : (
                <div className="placeholder">
                  Click 'Pull Screenshot' to fetch the active monitor
                </div>
              )}
            </div>
          </div>
        ))}
      </main>
    </div>
  );
}

export default App;
