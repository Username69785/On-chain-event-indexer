import { state, actions } from '../state.js';
import { renderMainArea } from './mainArea.js';

export function renderSidebar() {
    const addressList = document.getElementById('addressList');
    if (!addressList) return;
    addressList.innerHTML = '';
    
    state.addresses.forEach(item => {
        const el = document.createElement('div');
        el.className = `address-item ${state.activeAddress === item.address ? 'active' : ''}`;
        
        const shortAddr = `${item.address.slice(0, 4)}...${item.address.slice(-4)}`;
        
        const processed = item.processedTransactions ?? 0;
        const total = item.totalTransactions ?? 0;
        const percent = total > 0 ? Math.floor((processed / total) * 100) : 0;

        el.style.position = 'relative';
        el.style.flexDirection = 'column';
        el.style.alignItems = 'stretch';
        el.style.padding = '12px 16px';
        el.style.gap = '12px';

        el.innerHTML = `
            <div style="display: flex; justify-content: space-between; align-items: center;">
                <div class="address-item-content" style="gap: 8px;">
                    <span class="status-indicator ${item.status}"></span>
                    <span class="address-text" style="font-size: 1rem;">${shortAddr}</span>
                </div>
                <span class="percent-text" style="color:var(--text-secondary); font-size: 0.85rem; font-family:var(--font-mono); margin-right: 4px;">${percent}%</span>
                <button class="delete-btn" title="Remove address" style="position: absolute; right: 12px; top: 10px; background: var(--surface-color);">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M19 7L18.1327 19.1425C18.0579 20.1891 17.187 21 16.1378 21H7.86224C6.81296 21 5.94208 20.1891 5.86732 19.1425L5 7M10 11V17M14 11V17M15 7V4C15 3.44772 14.5523 3 14 3H10C9.44772 3 9 3.44772 9 4V7M4 7H20" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                </button>
            </div>
            <div style="height: 3px; background: rgba(255, 255, 255, 0.05); border-radius: 1.5px; overflow: hidden; width: 100%;">
                <div style="height: 100%; box-shadow: 0 0 8px rgba(59, 130, 246, 0.5); background: var(--accent-primary); width: ${percent}%; border-radius: 1.5px; transition: width 0.3s ease;"></div>
            </div>
        `;
        
        el.addEventListener('click', (e) => {
            if (e.target.closest('.delete-btn')) {
                actions.removeAddress(item.address);
            } else {
                actions.setActiveAddress(item.address);
            }
            renderSidebar();
            renderMainArea();
        });

        addressList.appendChild(el);
    });

    // Toggle sidebar visibility and layout based on address count
    const sidebar = document.getElementById('appSidebar');
    const appLayout = document.querySelector('.app-layout');
    const resizer = document.getElementById('appResizer');
    if (state.addresses.length > 0) {
        sidebar?.classList.remove('hidden');
        resizer?.classList.remove('hidden');
        appLayout?.classList.add('has-sidebar');
    } else {
        sidebar?.classList.add('hidden');
        resizer?.classList.add('hidden');
        appLayout?.classList.remove('has-sidebar');
    }
}
