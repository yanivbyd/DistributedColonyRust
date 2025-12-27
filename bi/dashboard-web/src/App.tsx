import { useState } from 'react';
import { ColonySelector } from './components/ColonySelector';
import { CreatureCoverageChart } from './components/CreatureCoverageChart';

function App() {
  const [selectedColony, setSelectedColony] = useState<string | null>(null);
  const [hideSelector, setHideSelector] = useState(false);

  const handleColoniesLoaded = (coloniesList: string[]) => {
    setHideSelector(coloniesList.length === 1);
  };

  return (
    <div className="bg-dark text-light" style={{ minHeight: '100vh' }}>
      <div className="container py-4">
        <h1 className="text-white mb-4">Colony BI Dashboard</h1>
        <ColonySelector 
          onColonySelect={setSelectedColony} 
          onColoniesLoaded={handleColoniesLoaded}
          hide={hideSelector}
        />
        <CreatureCoverageChart colonyId={selectedColony} />
      </div>
    </div>
  );
}

export default App;

