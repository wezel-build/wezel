import { lazy, Suspense } from "react";
import ScenariosPage from "./routes/ScenariosPage";
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

const router = createBrowserRouter([
  {
    path: "/",
    element: <Shell />,
    children: [
      {
        index: true,
        element: <ScenariosPage />,
      },
      {
        path: "project/:projectId",
        element: <ScenariosPage />,
      },
      {
        path: "project/:projectId/scenario/:id",
        element: <ScenariosPage />,
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
        path: "projects/create",
        element: (
          <Suspense>
            <NewProjectPage />
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
