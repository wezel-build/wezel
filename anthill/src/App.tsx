import { lazy, Suspense } from "react";
import ScenariosPage from "./routes/ScenariosPage";
import { ProjectProvider } from "./lib/ProjectContext";
import { createBrowserRouter, RouterProvider } from "react-router-dom";
import Shell from "./Shell";

const CommitPage = lazy(() => import("./routes/CommitPage"));
const MeasurementDetailPage = lazy(
  () => import("./routes/MeasurementDetailPage"),
);
const NewProjectPage = lazy(() => import("./routes/NewProjectPage"));

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
        path: "scenario/:id",
        element: <ScenariosPage />,
      },
      {
        path: "commit/:sha",
        element: (
          <Suspense>
            <CommitPage />
          </Suspense>
        ),
      },
      {
        path: "commit/:sha/m/:id",
        element: (
          <Suspense>
            <MeasurementDetailPage />
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

export default function App() {
  return (
    <ProjectProvider>
      <RouterProvider router={router} />
    </ProjectProvider>
  );
}
