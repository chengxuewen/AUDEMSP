import { Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import Dashboard from './pages/Dashboard';
import Settings from './pages/Settings';
import Audio from './pages/Audio';
import Vehicles from './pages/Vehicles';

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Dashboard />} />
        <Route path="/audio" element={<Audio />} />
        <Route path="/vehicles" element={<Vehicles />} />
        <Route path="/settings" element={<Settings />} />
      </Route>
    </Routes>
  );
}
