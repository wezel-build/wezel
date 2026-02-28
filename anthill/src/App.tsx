import { createBrowserRouter, RouterProvider } from "react-router-dom";
import Shell from "./Shell";
import ScenariosPage from "./routes/ScenariosPage";
import CommitPage from "./routes/CommitPage";
import MeasurementDetailPage from "./routes/MeasurementDetailPage";

const router = createBrowserRouter([
  {
    path: "/",
    element: <Shell />,
    children: [
      { index: true, element: <ScenariosPage /> },
      { path: "scenario/:id", element: <ScenariosPage /> },
      { path: "commit/:sha", element: <CommitPage /> },
      { path: "commit/:sha/m/:id", element: <MeasurementDetailPage /> },
    ],
  },
]);

export default function App() {
  return <RouterProvider router={router} />;
}
