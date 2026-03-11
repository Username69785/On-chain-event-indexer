import { state, getters } from '../state.js';
import { showStatus, showGlobalToast } from '../utils.js';

export function renderMainArea() {
    const mainContent = document.getElementById('mainContent');
    if (!mainContent) return;

    if (!state.activeAddress) {
        mainContent.innerHTML = `
            <div class="empty-state">
                <div class="empty-text">
                    <h3>No Address Selected</h3>
                    <p>Select an address from the sidebar or add a new one to start indexing on-chain events.</p>
                </div>
                <button id="emptyStateAddBtn" class="primary-btn mt-4">
                    <span>Add First Address</span>
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M12 5V19M5 12H19" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                </button>
            </div>
        `;
        
        const addBtn = document.getElementById('emptyStateAddBtn');
        if (addBtn) {
            addBtn.addEventListener('click', () => {
                const sidebarAddBtn = document.getElementById('addAddressBtn');
                if (sidebarAddBtn) sidebarAddBtn.click();
            });
        }
        return;
    }

    const currentItem = getters.getActiveItem();
    if (!currentItem) return;
    
    const shortAddr = `${currentItem.address.slice(0, 4)}...${currentItem.address.slice(-4)}`;
    const totalTransactions = currentItem.totalTransactions ?? 0;
    const processedTransactions = currentItem.processedTransactions ?? 0;
    const remainingTransactions = currentItem.remainingTransactions ?? 0;
    
    mainContent.innerHTML = `
        <div class="active-header" style="display:flex; flex-direction:column; margin-bottom:24px;">
            <div style="display:flex; gap:16px; align-items:center; margin-bottom: 8px;">
                <h1 title="${currentItem.address}" style="font-size: 2rem; font-family: var(--font-mono); font-weight: 700; color: var(--text-primary); cursor: default;">
                    ${shortAddr}
                </h1>
                
                <!-- Action Buttons right next to title -->
                <div style="display:flex; gap:8px;">
                    <button class="action-btn" id="copyBtn" title="Copy Address">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M8 16H6C4.89543 16 4 15.1046 4 14V6C4 4.89543 4.89543 4 6 4H14C15.1046 4 16 4.89543 16 6V8M18 8H10C8.89543 8 8 8.89543 8 10V18C8 19.1046 8.89543 20 10 20H18C19.1046 20 20 19.1046 20 18V10C20 8.89543 19.1046 8 18 8Z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                        </svg>
                    </button>
                    <a href="https://solscan.io/account/${currentItem.address}" target="_blank" class="action-btn" title="Open in Solscan">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M10 4H6C4.89543 4 4 4.89543 4 6V18C4 19.1046 4.89543 20 6 20H18C19.1046 20 20 19.1046 20 18V14M11 13L20 4M20 4H15M20 4V9" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                        </svg>
                    </a>
                </div>
            </div>

            <div style="display:flex; gap:12px; align-items:center;">
                <span class="status-indicator ${currentItem.status}"></span>
                <span style="color:var(--text-secondary); font-size:0.95rem; text-transform:capitalize;">Status: ${currentItem.status}</span>
            </div>
        </div>
        
        <div style="display: flex; flex-direction: column; gap: 24px;">
            <div class="card glass-effect" style="padding: 24px; border-radius: 16px; flex: 1; min-height: 200px;">
                <p style="color: var(--text-secondary); margin-bottom: 12px;">Total transactions: ${totalTransactions}</p>
                <p style="color: var(--text-secondary); margin-bottom: 12px;">Processed transactions: ${processedTransactions}</p>
                <p style="color: var(--text-secondary); margin-bottom: 12px;">Unprocessed transactions: ${remainingTransactions}</p>
                <p style="color: var(--text-secondary); margin-bottom: 0;">Current status: ${currentItem.status}</p>
            </div>
        </div>
    `;

    const copyBtn = document.getElementById('copyBtn');
    if (copyBtn) {
        copyBtn.addEventListener('click', () => {
            navigator.clipboard.writeText(currentItem.address)
                .then(() => showGlobalToast('Address copied!'))
                .catch(err => console.error('Copy failed', err));
        });
    }
}
