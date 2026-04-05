export function initResizer() {
    const resizer = document.getElementById('appResizer');
    const appLayout = document.querySelector('.app-layout');
    
    if (!resizer || !appLayout) return;

    let isResizing = false;

    resizer.addEventListener('mousedown', (e) => {
        isResizing = true;
        appLayout.classList.add('is-resizing');
        document.body.style.cursor = 'col-resize';
        e.preventDefault();
    });

    document.addEventListener('mousemove', (e) => {
        if (!isResizing) return;
        
        let newWidth = e.clientX;
        
        if (newWidth < 220) newWidth = 220;
        if (newWidth > 600) newWidth = 600;
        
        document.documentElement.style.setProperty('--sidebar-width', `${newWidth}px`);
    });

    document.addEventListener('mouseup', () => {
        if (isResizing) {
            isResizing = false;
            appLayout.classList.remove('is-resizing');
            document.body.style.cursor = '';
        }
    });
}
