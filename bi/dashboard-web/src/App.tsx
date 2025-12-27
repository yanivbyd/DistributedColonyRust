import { useState } from 'react';
import { ColonySelector } from './components/ColonySelector';
import { CreatureCoverageChart } from './components/CreatureCoverageChart';

function App() {
  const [selectedColony, setSelectedColony] = useState<string | null>(null);

  return (
    <div className="bg-dark text-light" style={{ minHeight: '100vh' }}>
      <div className="container py-4">
        <h1 className="text-white mb-4">Colony BI Dashboard</h1>
        <ColonySelector onColonySelect={setSelectedColony} />
        <CreatureCoverageChart colonyId={selectedColony} />
      </div>
    </div>
  );
}

export default App;

