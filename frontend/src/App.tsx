import { createBrowserRouter, RouterProvider } from "react-router-dom";
import Shell from "./Shell";
import ScenariosPage from "./routes/ScenariosPage";

const router = createBrowserRouter([
  {
    path: "/",
    element: <Shell />,
    children: [
      { index: true, element: <ScenariosPage /> },
      { path: "scenario/:id", element: <ScenariosPage /> },
    ],
  },
]);

export default function App() {
  return <RouterProvider router={router} />;
}
