graph TD
    A[Start App] --> B{Health Check}
    B -- Healthy --> C[Dashboard]
    B -- Missing Deps/Keys --> D[Show Health Warning in Dashboard]
    D --> E[User clicks Fix System Environment]
    E --> F[Show Sudo Explanation Modal]
    F -- Cancel --> C
    F -- Proceed --> G[Trigger pkexec with generated command]
    G -- Success --> H[Refresh Health Check]
    G -- Failure --> I[Show Error Message]
    H --> C
