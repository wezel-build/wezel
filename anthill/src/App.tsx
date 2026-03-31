import { lazy, Suspense } from "react";
import ObservationsPage from "./routes/ObservationsPage";
import { ProjectProvider } from "./lib/ProjectContext";
import { createBrowserRouter, RouterProvider } from "react-router-dom";
import Shell from "./Shell";
import { AuthProvider, useAuth } from "./lib/AuthContext";
import LoginPage from "./routes/LoginPage";

const CommitPage = lazy(() => import("./routes/CommitPage"));
const MeasurementDetailPage = lazy(
  () => import("./routes/MeasurementDetailPage"),
);
const NewProjectPage = lazy(() => import("./routes/NewProjectPage"));
const CommitsListPage = lazy(() => import("./routes/CommitsListPage"));
const PheromoneAdminPage = lazy(() => import("./routes/PheromoneAdminPage"));

import TimelinePage from "./routes/TimelinePage";
import ComparePage from "./routes/ComparePage";
import BisectionsPage from "./routes/BisectionsPage";
import BisectionDetailPage from "./routes/BisectionDetailPage";

const router = createBrowserRouter([
  {
    path: "/",
    element: <Shell />,
    children: [
      {
        index: true,
        element: (
          <Suspense>
            <CommitsListPage />
          </Suspense>
        ),
      },
      {
        path: "project/:projectId",
        element: (
          <Suspense>
            <CommitsListPage />
          </Suspense>
        ),
      },
      {
        path: "project/:projectId/observations",
        element: <ObservationsPage />,
      },
      {
        path: "project/:projectId/observation/:id",
        element: <ObservationsPage />,
      },
      {
        path: "project/:projectId/commit/:sha",
        element: (
          <Suspense>
            <CommitPage />
          </Suspense>
        ),
      },
      {
        path: "project/:projectId/commit/:sha/m/:id",
        element: (
          <Suspense>
            <MeasurementDetailPage />
          </Suspense>
        ),
      },
      {
        path: "project/:projectId/commits",
        element: (
          <Suspense>
            <CommitsListPage />
          </Suspense>
        ),
      },
      {
        path: "project/:projectId/branch/:branch/timeline",
        element: <TimelinePage />,
      },
      {
        path: "project/:projectId/compare",
        element: <ComparePage />,
      },
      {
        path: "project/:projectId/bisections",
        element: <BisectionsPage />,
      },
      {
        path: "project/:projectId/bisections/:bisectionId",
        element: <BisectionDetailPage />,
      },
      {
        path: "projects/create",
        element: (
          <Suspense>
            <NewProjectPage />
          </Suspense>
        ),
      },
      {
        path: "admin/pheromones",
        element: (
          <Suspense>
            <PheromoneAdminPage />
          </Suspense>
        ),
      },
    ],
  },
]);

function AuthGate() {
  const { user, loading, forbidden, authRequired } = useAuth();
  if (loading) return null;
  if (!user && authRequired) return <LoginPage forbidden={forbidden} />;
  return (
    <ProjectProvider>
      <RouterProvider router={router} />
    </ProjectProvider>
  );
}

export default function App() {
  return (
    <AuthProvider>
      <AuthGate />
    </AuthProvider>
  );
}
